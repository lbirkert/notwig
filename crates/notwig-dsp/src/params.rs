use notwig_core::{Db, Norm01, ParamId, ParamValue};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ParamKind {
    Continuous,
    Discrete,
    Bool,
    Enum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Unit {
    None,
    Decibels,
    Hertz,
    Percent,
    Seconds,
    Semitones,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RangeSpec {
    pub min: f64,
    pub max: f64,
    pub step: Option<f64>,
    pub enum_values: Option<u32>,
}

impl RangeSpec {
    #[must_use]
    pub const fn continuous(min: f64, max: f64) -> Self {
        Self {
            min,
            max,
            step: None,
            enum_values: None,
        }
    }

    #[must_use]
    pub const fn stepped(min: f64, max: f64, step: f64) -> Self {
        Self {
            min,
            max,
            step: Some(step),
            enum_values: None,
        }
    }

    #[must_use]
    pub const fn enumeration(values: u32) -> Self {
        Self {
            min: 0.0,
            max: (values.saturating_sub(1)) as f64,
            step: Some(1.0),
            enum_values: Some(values),
        }
    }

    #[must_use]
    pub fn clamp(&self, value: f64) -> f64 {
        value.max(self.min).min(self.max)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NormMap {
    pub min: f64,
    pub max: f64,
    pub logarithmic: bool,
    pub stepped: bool,
}

impl NormMap {
    #[must_use]
    pub const fn linear(min: f64, max: f64) -> Self {
        Self {
            min,
            max,
            logarithmic: false,
            stepped: false,
        }
    }

    #[must_use]
    pub const fn logarithmic(min: f64, max: f64) -> Self {
        Self {
            min,
            max,
            logarithmic: true,
            stepped: true,
        }
    }

    #[must_use]
    pub fn to_norm(&self, physical: f64) -> Norm01 {
        let value = if self.logarithmic {
            let min = self.min.max(f64::MIN_POSITIVE);
            let max = self.max.max(min * (1.0 + f64::EPSILON));
            let clamped = physical.max(min).min(max);
            (clamped.ln() - min.ln()) / (max.ln() - min.ln())
        } else if (self.max - self.min).abs() <= f64::EPSILON {
            0.0
        } else {
            (physical - self.min) / (self.max - self.min)
        };
        Norm01::clamped(value as f32)
    }

    #[must_use]
    pub fn to_physical(&self, norm: f32) -> f64 {
        let norm = f64::from(norm.clamp(0.0, 1.0));
        if self.logarithmic {
            let min = self.min.max(f64::MIN_POSITIVE);
            let max = self.max.max(min * (1.0 + f64::EPSILON));
            (min.ln() + norm * (max.ln() - min.ln())).exp()
        } else {
            self.min + norm * (self.max - self.min)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SmoothingSpec {
    pub milliseconds: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ParamFlags {
    pub automatable: bool,
    pub visible: bool,
    pub modulatable_reserved: bool,
    pub macro_assignable_reserved: bool,
}

impl Default for ParamFlags {
    fn default() -> Self {
        Self {
            automatable: true,
            visible: true,
            modulatable_reserved: true,
            macro_assignable_reserved: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ParamSpec {
    pub id: ParamId,
    pub name: String,
    pub kind: ParamKind,
    pub unit: Unit,
    pub range: RangeSpec,
    pub default: ParamValue,
    pub norm_map: NormMap,
    pub smoothing: Option<SmoothingSpec>,
    pub flags: ParamFlags,
}

impl ParamSpec {
    #[must_use]
    pub fn continuous(
        id: ParamId,
        name: impl Into<String>,
        unit: Unit,
        min: f64,
        max: f64,
        default: f64,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            kind: ParamKind::Continuous,
            unit,
            range: RangeSpec::continuous(min, max),
            default: ParamValue::F64(default),
            norm_map: if unit == Unit::Hertz {
                NormMap::logarithmic(min, max)
            } else {
                NormMap::linear(min, max)
            },
            smoothing: None,
            flags: ParamFlags::default(),
        }
    }

    #[must_use]
    pub fn boolean(id: ParamId, name: impl Into<String>, default: bool) -> Self {
        Self {
            id,
            name: name.into(),
            kind: ParamKind::Bool,
            unit: Unit::None,
            range: RangeSpec::stepped(0.0, 1.0, 1.0),
            default: ParamValue::Bool(default),
            norm_map: NormMap::linear(0.0, 1.0),
            smoothing: None,
            flags: ParamFlags::default(),
        }
    }

    #[must_use]
    pub fn enumeration(id: ParamId, name: impl Into<String>, values: u32, default: u32) -> Self {
        Self {
            id,
            name: name.into(),
            kind: ParamKind::Enum,
            unit: Unit::None,
            range: RangeSpec::enumeration(values),
            default: ParamValue::EnumIndex(default.min(values.saturating_sub(1))),
            norm_map: NormMap::linear(0.0, f64::from(values.saturating_sub(1))),
            smoothing: None,
            flags: ParamFlags::default(),
        }
    }

    #[must_use]
    pub fn physical(&self, value: &ParamValue) -> f64 {
        match (self.kind, value) {
            (ParamKind::Bool, ParamValue::Bool(v)) => f64::from(u8::from(*v)),
            (_, value) => self.range.clamp(value.as_f64().unwrap_or(0.0)),
        }
    }

    #[must_use]
    pub fn physical_db(&self, value: &ParamValue) -> Db {
        Db(self.physical(value) as f32)
    }

    #[must_use]
    pub fn norm(&self, value: &ParamValue) -> Norm01 {
        self.norm_map.to_norm(self.physical(value))
    }

    #[must_use]
    pub fn value_from_norm(&self, norm: Norm01) -> ParamValue {
        match self.kind {
            ParamKind::Bool => ParamValue::Bool(norm.0 >= 0.5),
            ParamKind::Enum | ParamKind::Discrete => ParamValue::EnumIndex(
                self.norm_map
                    .to_physical(norm.0)
                    .round()
                    .clamp(self.range.min, self.range.max) as u32,
            ),
            ParamKind::Continuous => ParamValue::F64(self.norm_map.to_physical(norm.0)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Value {
    Phys(f64),
    Norm(Norm01),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ParamSet {
    pub device_id: notwig_core::DeviceId,
    pub param_id: ParamId,
    pub value: Value,
    pub sample_offset: Option<u32>,
    pub ramp: Option<notwig_core::protocol::RampSpec>,
}
