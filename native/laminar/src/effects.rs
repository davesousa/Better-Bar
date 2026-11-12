//! Composition effects (the GPU effect graph behind Windows' Acrylic): extra blur on the
//! glass.
//!
//! Tried and refused by Windows (2026-11-10), so the prototype's refraction can't be done
//! on the live blurred desktop: a 2D transform ("Backdrop brushes cannot be transformed")
//! and Direct2D's displacement map ("Unsupported effect type").
//!
//! Windows.UI.Composition takes an effect description object that implements
//! `IGraphicsEffect` plus `IGraphicsEffectD2D1Interop`, which names a Direct2D effect
//! (by CLSID) and hands over its properties by index. Win2D usually provides those
//! objects; this small implementation covers the few effects Laminar needs.

use windows::core::{implement, Interface, Result, GUID, HSTRING, PCWSTR};
use windows::Foundation::{IPropertyValue, PropertyValue};
use windows::Graphics::Effects::{IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffect_Impl, IGraphicsEffectSource_Impl};
use windows::UI::Composition::*;
use windows::Win32::Graphics::Direct2D::CLSID_D2D1GaussianBlur;
use windows::Win32::System::WinRT::Graphics::Direct2D::{
    IGraphicsEffectD2D1Interop, IGraphicsEffectD2D1Interop_Impl, GRAPHICS_EFFECT_PROPERTY_MAPPING,
};

#[derive(Clone)]
enum Prop {
    F32(f32),
    U32(u32),
}

#[implement(IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectD2D1Interop)]
struct Effect {
    clsid: GUID,
    props: Vec<Prop>,
    sources: Vec<IGraphicsEffectSource>,
    name: std::cell::RefCell<HSTRING>,
}

impl IGraphicsEffect_Impl for Effect_Impl {
    fn Name(&self) -> Result<HSTRING> {
        Ok(self.name.borrow().clone())
    }
    fn SetName(&self, name: &HSTRING) -> Result<()> {
        *self.name.borrow_mut() = name.clone();
        Ok(())
    }
}

impl IGraphicsEffectSource_Impl for Effect_Impl {}

impl IGraphicsEffectD2D1Interop_Impl for Effect_Impl {
    fn GetEffectId(&self) -> Result<GUID> {
        Ok(self.clsid)
    }
    fn GetNamedPropertyMapping(&self, _name: &PCWSTR, _index: *mut u32, _mapping: *mut GRAPHICS_EFFECT_PROPERTY_MAPPING) -> Result<()> {
        // No animatable properties by name.
        Err(windows::Win32::Foundation::E_INVALIDARG.into())
    }
    fn GetPropertyCount(&self) -> Result<u32> {
        Ok(self.props.len() as u32)
    }
    fn GetProperty(&self, index: u32) -> Result<IPropertyValue> {
        let v = match self.props.get(index as usize) {
            Some(Prop::F32(f)) => PropertyValue::CreateSingle(*f)?,
            Some(Prop::U32(u)) => PropertyValue::CreateUInt32(*u)?,
            None => return Err(windows::Win32::Foundation::E_BOUNDS.into()),
        };
        v.cast()
    }
    fn GetSource(&self, index: u32) -> Result<IGraphicsEffectSource> {
        self.sources.get(index as usize).cloned().ok_or_else(|| windows::Win32::Foundation::E_BOUNDS.into())
    }
    fn GetSourceCount(&self) -> Result<u32> {
        Ok(self.sources.len() as u32)
    }
}

fn effect(clsid: GUID, props: Vec<Prop>, sources: Vec<IGraphicsEffectSource>) -> IGraphicsEffect {
    Effect { clsid, props, sources, name: std::cell::RefCell::new(HSTRING::new()) }.into()
}

fn source(name: &str) -> Result<IGraphicsEffectSource> {
    CompositionEffectSourceParameter::Create(&HSTRING::from(name))?.cast()
}

/// Gaussian blur with hard edges (the edge pixels repeat instead of fading out).
fn blur(input: IGraphicsEffectSource, std_dev: f32) -> IGraphicsEffect {
    // D2D1_GAUSSIANBLUR_PROP: 0 StandardDeviation, 1 Optimization (2 = quality), 2 BorderMode (1 = hard).
    effect(CLSID_D2D1GaussianBlur, vec![Prop::F32(std_dev), Prop::U32(2), Prop::U32(1)], vec![input])
}

/// The live blurred desktop (`backdrop`), blurred further by `std_dev` pixels.
pub fn blurred_backdrop(c: &Compositor, backdrop: &CompositionBrush, std_dev: f32) -> Result<CompositionBrush> {
    let e = blur(source("bd")?, std_dev);
    let brush = c.CreateEffectFactory(&e)?.CreateBrush()?;
    brush.SetSourceParameter(&HSTRING::from("bd"), backdrop)?;
    brush.cast()
}

