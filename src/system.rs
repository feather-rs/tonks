use crate::resources::Resource;
use crate::scheduler::TaskMessage;
use crate::{mappings::Mappings, resource_id_for, ResourceId, Resources};
use bumpalo::Bump;
use crossbeam::Sender;
use lazy_static::lazy_static;
use parking_lot::Mutex;
use std::any::TypeId;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use thread_local::ThreadLocal;

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
    unsafe fn execute_raw(&mut self, resources: &Resources, ctx: SystemCtx);
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

    unsafe fn execute_raw(&mut self, resources: &Resources, ctx: SystemCtx) {
        let data = self
            .data
            .get_or_insert_with(|| S::SystemData::load_from_resources(resources, ctx));

        self.inner.run(data);
    }
}

/// Context of a running system, immutable across runs.
#[derive(Clone)]
pub struct SystemCtx {
    /// Sender to the scheduler.
    pub(crate) sender: Sender<TaskMessage>,
    /// ID of this system.
    pub(crate) id: SystemId,
    pub(crate) bump: Arc<ThreadLocal<Bump>>,
}

/// One or more resources in a tuple.
pub trait SystemData: Send + Sync {
    fn reads() -> Vec<ResourceId>;
    fn writes() -> Vec<ResourceId>;

    /// Loads this `SystemData` from the provided `Resources`.
    ///
    /// # Safety
    /// Only resources returned by `reads()` and `writes()` may be accessed.
    unsafe fn load_from_resources(resources: &Resources, ctx: SystemCtx) -> Self;

    /// Called at the end of every system execution.
    ///
    /// The default implementation of this function is a no-op.
    fn flush(&mut self) {}
}

impl SystemData for () {
    fn reads() -> Vec<ResourceId> {
        vec![]
    }

    fn writes() -> Vec<ResourceId> {
        vec![]
    }

    unsafe fn load_from_resources(_resources: &Resources, _ctx: SystemCtx) -> Self {}
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

    unsafe fn load_from_resources(resources: &Resources, _ctx: SystemCtx) -> Self {
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

    unsafe fn load_from_resources(resources: &Resources, _ctx: SystemCtx) -> Self {
        Self {
            ptr: resources.get_mut(resource_id_for::<T>()) as *mut T,
        }
    }
}

macro_rules! impl_data {
    ( $($ty:ident),* ; $($idx:tt),*) => {
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

            unsafe fn load_from_resources(resources: &Resources, ctx: SystemCtx) -> Self {
                ($($ty::load_from_resources(resources, ctx.clone()) ,)*)
            }

            fn flush(&mut self) {
                $(self.$idx.flush() ;)*
            }
        }
    }
}

// Generated by https://play.rust-lang.org/?version=stable&mode=debug&edition=2018&gist=d39eca91f8762c1563956d2745401ce9
impl_data!(A; 0);
impl_data!(A, B; 0, 1);
impl_data!(A, B, C; 0, 1, 2);
impl_data!(A, B, C, D; 0, 1, 2, 3);
impl_data!(A, B, C, D, E; 0, 1, 2, 3, 4);
impl_data!(A, B, C, D, E, F; 0, 1, 2, 3, 4, 5);
impl_data!(A, B, C, D, E, F, G; 0, 1, 2, 3, 4, 5, 6);
impl_data!(A, B, C, D, E, F, G, H; 0, 1, 2, 3, 4, 5, 6, 7);
impl_data!(A, B, C, D, E, F, G, H, I; 0, 1, 2, 3, 4, 5, 6, 7, 8);
impl_data!(A, B, C, D, E, F, G, H, I, J; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9);
impl_data!(A, B, C, D, E, F, G, H, I, J, K; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24);
