use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

pub trait Entity {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Asset;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Track;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Clip;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Note;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Device;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct DeviceChain;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Param;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Send;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Slice;

impl Entity for Asset {}
impl Entity for Track {}
impl Entity for Clip {}
impl Entity for Note {}
impl Entity for Device {}
impl Entity for DeviceChain {}
impl Entity for Param {}
impl Entity for Send {}
impl Entity for Slice {}

pub type AssetId = Id<Asset>;
pub type TrackId = Id<Track>;
pub type ClipId = Id<Clip>;
pub type NoteId = Id<Note>;
pub type DeviceId = Id<Device>;
pub type DeviceChainId = Id<DeviceChain>;
pub type ParamId = Id<Param>;
pub type SendId = Id<Send>;
pub type SliceId = Id<Slice>;

pub struct Id<T: ?Sized> {
    uuid: Uuid,
    marker: PhantomData<fn() -> T>,
}

impl<T: ?Sized> Id<T> {
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self {
            uuid,
            marker: PhantomData,
        }
    }

    #[must_use]
    pub const fn nil() -> Self {
        Self::from_uuid(Uuid::nil())
    }

    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.uuid
    }

    #[must_use]
    pub fn into_uuid(self) -> Uuid {
        self.uuid
    }
}

impl<T: ?Sized> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: ?Sized> Copy for Id<T> {}

impl<T: ?Sized> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.uuid == other.uuid
    }
}

impl<T: ?Sized> Eq for Id<T> {}

impl<T: ?Sized> PartialOrd for Id<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: ?Sized> Ord for Id<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.uuid.cmp(&other.uuid)
    }
}

impl<T: ?Sized> std::hash::Hash for Id<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.uuid.hash(state);
    }
}

impl<T: ?Sized> Default for Id<T> {
    fn default() -> Self {
        Self::nil()
    }
}

impl<T: ?Sized> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.uuid)
    }
}

impl<T: ?Sized> fmt::Display for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.uuid)
    }
}

impl<T: ?Sized> FromStr for Id<T> {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from_uuid(Uuid::from_str(s)?))
    }
}

impl<T: ?Sized> Serialize for Id<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.uuid.to_string())
    }
}

impl<'de, T: ?Sized> Deserialize<'de> for Id<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        let uuid = Uuid::parse_str(&raw).map_err(D::Error::custom)?;
        Ok(Self::from_uuid(uuid))
    }
}

#[macro_export]
macro_rules! test_id {
    ($ty:ty, $n:expr) => {
        $crate::ids::Id::<$ty>::from_uuid(uuid::Uuid::from_u128($n))
    };
}
