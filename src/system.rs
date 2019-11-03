use crate::resources::Resource;
use crate::{mappings::Mappings, resource_id_for, ResourceId, Resources};
use lazy_static::lazy_static;
use parking_lot::Mutex;
use std::any::TypeId;
use std::ops::{Deref, DerefMut};

/// Unique ID of a system, allocated consecutively for use as indices into vectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct SystemId(pub usize);

impl From<usize> for SystemId {
    fn from(x: usize) -> Self {
        Self(x)
    }
}

lazy_static! {
    /// Mappings from `TypeId`s to `SystemId`s.
    pub static ref SYSTEM_ID_MAPPINGS: Mutex<Mappings<TypeId, SystemId>> = Mutex::new(Mappings::new());
}

/// Returns the system ID corresponding to the given type.
pub fn system_id_for<T: 'static>() -> SystemId {
    SYSTEM_ID_MAPPINGS.lock().get_or_alloc(TypeId::of::<T>())
}

/// A raw system, either a normal or one-shottable one.
///
/// Users should not use this type unless they know what they are doing.
/// The only case in which this trait will be useful is if advanced usage
/// is required, such as creating systems provided by scripts loaded at runtime.
pub trait RawSystem: Send + Sync {
    /// Returns the unique ID of this system, as allocated by `system_id_for::<T>()`.
    fn id(&self) -> SystemId;

    /// Returns the resources read by this system.
    fn resource_reads(&self) -> &[ResourceId];
    /// Returns the resources written by this system.
    fn resource_writes(&self) -> &[ResourceId];

    /// Runs this system, fetching any resources from the provided `Resources`.
    ///
    /// # Safety
    /// The system must not access any resources not indicated by `resource_reads()` and `resource_writes()`.
    unsafe fn execute_raw(&mut self, resources: &Resources);
}

// High-level system API

/// A system. TODO: docs
pub trait System: Send + Sync {
    type SystemData: SystemData;

    fn run(&mut self, data: &mut Self::SystemData);
}

pub struct CachedSystem<S: System> {
    inner: S,
    /// Cached system ID.
    pub(crate) id: SystemId,
    /// Cached resource reads.
    pub(crate) resource_reads: Vec<ResourceId>,
    /// Cached resource writes.
    pub(crate) resource_writes: Vec<ResourceId>,
    /// Cached system data, or `None` if it has not yet been loaded.
    pub(crate) data: Option<S::SystemData>,
}

impl<S: System + 'static> CachedSystem<S> {
    pub fn new(inner: S) -> Self {
        Self {
            id: SYSTEM_ID_MAPPINGS.lock().alloc(),
            resource_reads: S::SystemData::reads(),
            resource_writes: S::SystemData::writes(),
            data: None,
            inner,
        }
    }
}

impl<S: System> RawSystem for CachedSystem<S> {
    fn id(&self) -> SystemId {
        self.id
    }

    fn resource_reads(&self) -> &[ResourceId] {
        &self.resource_reads
    }

    fn resource_writes(&self) -> &[ResourceId] {
        &self.resource_writes
    }

    unsafe fn execute_raw(&mut self, resources: &Resources) {
        let data = self
            .data
            .get_or_insert_with(|| S::SystemData::load_from_resources(resources));

        self.inner.run(data);
    }
}

/// One or more resources in a tuple.
pub trait SystemData: Send + Sync {
    fn reads() -> Vec<ResourceId>;
    fn writes() -> Vec<ResourceId>;

    /// Loads this `SystemData` from the provided `Resources`.
    ///
    /// # Safety
    /// Only resources returned by `reads()` and `writes()` may be accessed.
    unsafe fn load_from_resources(resources: &Resources) -> Self;
}

impl SystemData for () {
    fn reads() -> Vec<ResourceId> {
        vec![]
    }

    fn writes() -> Vec<ResourceId> {
        vec![]
    }

    unsafe fn load_from_resources(_resources: &Resources) -> Self {}
}

/// Specifies a read requirement for a resource.
// Safety: this contains a raw pointer which must remain valid.
pub struct Read<T>
where
    T: Resource,
{
    ptr: *const T,
}

impl<T> Deref for Read<T>
where
    T: Resource,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.ptr }
    }
}

// Safety: raw pointers are valid as per the scheduler guarantees.
unsafe impl<T: Send + Resource> Send for Read<T> {}
unsafe impl<T: Send + Sync + Resource> Sync for Read<T> {}

impl<T> SystemData for Read<T>
where
    T: Resource,
{
    fn reads() -> Vec<ResourceId> {
        vec![resource_id_for::<T>()]
    }

    fn writes() -> Vec<ResourceId> {
        vec![]
    }

    unsafe fn load_from_resources(resources: &Resources) -> Self {
        Self {
            ptr: resources.get(resource_id_for::<T>()) as *const T,
        }
    }
}

/// Specifies a write requirement for a resource.
// Safety: this contains a raw pointer which must remain valid.
pub struct Write<T>
where
    T: Resource,
{
    ptr: *mut T,
}

impl<T> Deref for Write<T>
where
    T: Resource,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.ptr }
    }
}

impl<T> DerefMut for Write<T>
where
    T: Resource,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.ptr }
    }
}

// Safety: raw pointers are valid as per the scheduler guarantees.
unsafe impl<T: Send + Resource> Send for Write<T> {}
unsafe impl<T: Send + Sync + Resource> Sync for Write<T> {}

impl<T> SystemData for Write<T>
where
    T: Resource,
{
    fn reads() -> Vec<ResourceId> {
        vec![]
    }

    fn writes() -> Vec<ResourceId> {
        vec![resource_id_for::<T>()]
    }

    unsafe fn load_from_resources(resources: &Resources) -> Self {
        Self {
            ptr: resources.get_mut(resource_id_for::<T>()) as *mut T,
        }
    }
}

macro_rules! impl_data {
    ( $($ty:ident),* ) => {
        impl <$($ty),*> SystemData for ($($ty,)*) where $($ty: SystemData),* {
            fn reads() -> Vec<ResourceId> {
                let mut res = vec![];
                $(
                    res.append(&mut $ty::reads());
                )*
                res
            }

            fn writes() -> Vec<ResourceId> {
                let mut res = vec![];
                $(
                    res.append(&mut $ty::writes());
                )*
                res
            }

            unsafe fn load_from_resources(resources: &Resources) -> Self {
                ($($ty::load_from_resources(resources) ,)*)
            }
        }
    }
}

impl_data!(A);
impl_data!(A, B);
impl_data!(A, B, C);
impl_data!(A, B, C, D);
impl_data!(A, B, C, D, E);
impl_data!(A, B, C, D, E, F);
impl_data!(A, B, C, D, E, F, G);
impl_data!(A, B, C, D, E, F, G, H);
impl_data!(A, B, C, D, E, F, G, H, I);
impl_data!(A, B, C, D, E, F, G, H, I, J);
impl_data!(A, B, C, D, E, F, G, H, I, J, K);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z);
