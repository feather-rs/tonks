use crate::resources::Resource;
use crate::scheduler::TaskMessage;
use crate::{mappings::Mappings, resource_id_for, ResourceId, Resources, TryDefault};
use bumpalo::Bump;
use crossbeam::Sender;
use lazy_static::lazy_static;
use legion::storage::ComponentTypeId;
use legion::world::World;
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

    /// Returns the name of this system.
    fn name(&self) -> &'static str;

    /// Returns the resources read by this system.
    fn resource_reads(&self) -> &[ResourceId];
    /// Returns the resources written by this system.
    fn resource_writes(&self) -> &[ResourceId];
    /// Returns the components read by this system.
    fn component_reads(&self) -> &[ComponentTypeId];
    /// Returns the components written by this system.
    fn component_writes(&self) -> &[ComponentTypeId];

    /// Initializes this system, inserting any necessary resources.
    fn init(&mut self, resources: &mut Resources, ctx: SystemCtx, world: &World);

    /// Runs this system, fetching any resources from the provided `Resources`.
    ///
    /// # Safety
    /// The system must not access any resources not indicated by `resource_reads()` and `resource_writes()`.
    unsafe fn execute_raw(&mut self, resources: &Resources, ctx: SystemCtx, world: &World);
}

// High-level system API

/// A system. TODO: docs
pub trait System: Send + Sync + 'static {
    type SystemData: for<'a> SystemData<'a>;

    fn run(&mut self, data: <Self::SystemData as SystemData>::Output);
}

pub struct CachedSystem<S: System> {
    inner: S,
    /// Cached system ID.
    pub(crate) id: SystemId,
    /// Cached resource reads.
    pub(crate) resource_reads: Vec<ResourceId>,
    /// Cached resource writes.
    pub(crate) resource_writes: Vec<ResourceId>,
    /// Cached component reads.
    pub(crate) component_reads: Vec<ComponentTypeId>,
    /// Cached component writes.
    pub(crate) component_writes: Vec<ComponentTypeId>,
    /// Cached system data, or `None` if it has not yet been loaded.
    pub(crate) data: Option<S::SystemData>,
    pub(crate) name: &'static str,
}

impl<S: System + 'static> CachedSystem<S> {
    pub fn new(inner: S, name: &'static str) -> Self {
        Self {
            id: SYSTEM_ID_MAPPINGS.lock().alloc(),
            resource_reads: S::SystemData::resource_reads(),
            resource_writes: S::SystemData::resource_writes(),
            component_reads: S::SystemData::component_reads(),
            component_writes: S::SystemData::component_writes(),
            data: None,
            inner,
            name,
        }
    }
}

impl<S: System> RawSystem for CachedSystem<S> {
    fn id(&self) -> SystemId {
        self.id
    }

    fn name(&self) -> &'static str {
        self.name
    }

    fn resource_reads(&self) -> &[ResourceId] {
        &self.resource_reads
    }

    fn resource_writes(&self) -> &[ResourceId] {
        &self.resource_writes
    }

    fn component_reads(&self) -> &[ComponentTypeId] {
        &self.component_reads
    }

    fn component_writes(&self) -> &[ComponentTypeId] {
        &self.component_writes
    }

    fn init(&mut self, resources: &mut Resources, ctx: SystemCtx, world: &World) {
        let mut data = unsafe { S::SystemData::load_from_resources(resources, ctx, world) };
        data.init(resources, &self.component_reads, &self.component_writes);
        self.data = Some(data);
    }

    unsafe fn execute_raw(&mut self, _resources: &Resources, _ctx: SystemCtx, _world: &World) {
        let data = self.data.as_mut().unwrap();

        self.inner.run(data.before_execution());

        data.after_execution();
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

/// A system data type. This could include queries, event triggers, `PreparedWorld`, resource
/// access, and tuples of `SystemData`. Users may also implement their own custom `SystemData`
/// if needed.
pub trait SystemData<'a>: Send + Sync + Sized + 'a {
    /// Type which is actually passed to systems.
    type Output: SystemDataOutput<'a, SystemData = Self> + 'a;

    /// Loads this `SystemData` from the provided `Resources`
    /// and `legion::World`.
    ///
    /// # Safety
    /// Only resources returned by `reads()` and `writes()` may be accessed.
    unsafe fn load_from_resources(resources: &mut Resources, ctx: SystemCtx, world: &World)
        -> Self;

    /// Initializes this `SystemData`. This function is called
    /// before the system is ever run but after `load_from_resources()`.
    ///
    /// This function is called after all system data for a system
    /// has been loaded using `load_from_resources()`. As a result,
    /// this function can utilize data such as the component and resource
    /// accesses required by other system data.
    fn init(
        &mut self,
        _resources: &mut Resources,
        _component_reads: &[ComponentTypeId],
        _component_writes: &[ComponentTypeId],
    ) {
    }

    fn resource_reads() -> Vec<ResourceId>;
    fn resource_writes() -> Vec<ResourceId>;

    fn component_reads() -> Vec<ComponentTypeId>;
    fn component_writes() -> Vec<ComponentTypeId>;

    /// Prepares this `SystemData`, returning `Self::Output`
    /// to pass to a system.
    ///
    /// This function is called before every system execution.
    fn before_execution(&'a mut self) -> Self::Output;

    /// Called at the end of every system execution.
    ///
    /// The default implementation of this function is a no-op.
    fn after_execution(&mut self) {}
}

/// Output of a `SystemData`.
pub trait SystemDataOutput<'a>: Sized + 'a {
    type SystemData: SystemData<'a, Output = Self> + 'a;
}

impl<'a> SystemData<'a> for () {
    type Output = Self;

    unsafe fn load_from_resources(
        _resources: &mut Resources,
        _ctx: SystemCtx,
        _world: &World,
    ) -> Self {
    }

    fn resource_reads() -> Vec<ResourceId> {
        vec![]
    }

    fn resource_writes() -> Vec<ResourceId> {
        vec![]
    }

    fn component_reads() -> Vec<ComponentTypeId> {
        vec![]
    }

    fn component_writes() -> Vec<ComponentTypeId> {
        vec![]
    }

    fn before_execution(&'a mut self) -> Self::Output {}
}

impl<'a> SystemDataOutput<'a> for () {
    type SystemData = Self;
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

impl<'a, T> SystemData<'a> for Read<T>
where
    T: Resource + TryDefault,
{
    type Output = &'a mut Self;

    unsafe fn load_from_resources(
        resources: &mut Resources,
        _ctx: SystemCtx,
        _world: &World,
    ) -> Self {
        if let Some(default) = T::try_default() {
            resources.insert_if_absent(default);
        }

        Self {
            ptr: resources.get_unchecked(resource_id_for::<T>()) as *const T,
        }
    }

    fn resource_reads() -> Vec<ResourceId> {
        vec![resource_id_for::<T>()]
    }

    fn resource_writes() -> Vec<ResourceId> {
        vec![]
    }

    fn component_reads() -> Vec<ComponentTypeId> {
        vec![]
    }

    fn component_writes() -> Vec<ComponentTypeId> {
        vec![]
    }

    fn before_execution(&'a mut self) -> Self::Output {
        self
    }
}

impl<'a, T> SystemDataOutput<'a> for &'a mut Read<T>
where
    T: Resource + TryDefault,
{
    type SystemData = Read<T>;
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

impl<'a, T> SystemData<'a> for Write<T>
where
    T: Resource + TryDefault,
{
    type Output = &'a mut Self;

    unsafe fn load_from_resources(
        resources: &mut Resources,
        _ctx: SystemCtx,
        _world: &World,
    ) -> Self {
        if let Some(default) = T::try_default() {
            resources.insert_if_absent(default);
        }

        Self {
            ptr: resources.get_mut_unchecked(resource_id_for::<T>()) as *mut T,
        }
    }

    fn init(
        &mut self,
        resources: &mut Resources,
        _component_reads: &[ComponentTypeId],
        _component_writes: &[ComponentTypeId],
    ) {
        if let Some(default) = T::try_default() {
            resources.insert_if_absent(default);
        }
    }

    fn resource_reads() -> Vec<ResourceId> {
        vec![]
    }

    fn resource_writes() -> Vec<ResourceId> {
        vec![resource_id_for::<T>()]
    }

    fn component_reads() -> Vec<ComponentTypeId> {
        vec![]
    }

    fn component_writes() -> Vec<ComponentTypeId> {
        vec![]
    }

    fn before_execution(&'a mut self) -> Self::Output {
        self
    }
}

impl<'a, T> SystemDataOutput<'a> for &'a mut Write<T>
where
    T: Resource + TryDefault,
{
    type SystemData = Write<T>;
}

// `system` macro implementation details.
// This is used to allow for custom SystemData impls
// which don't go through `Read` and `Write`.

pub trait MacroData: 'static + Send + Sync {
    type SystemData: for<'a> SystemData<'a>;
}

// System data tuple impls.

macro_rules! impl_data {
    ( $($ty:ident),* ; $($idx:tt),*) => {
        impl <'a, $($ty),*> MacroData for ($($ty,)*) where $($ty: MacroData),* {
            type SystemData = ($($ty::SystemData, )*);
        }

        impl <'a, $($ty),*> SystemData<'a> for ($($ty,)*) where $($ty: SystemData<'a>),* {
            type Output = ($($ty::Output ,)*);

            fn before_execution(&'a mut self) -> Self::Output {
                ($(self.$idx.before_execution() ,)*)
            }

            fn init(&mut self, resources: &mut Resources, component_reads: &[ComponentTypeId], component_writes: &[ComponentTypeId]) {
                $(self.$idx.init(resources, component_reads, component_writes); )*
            }

            fn resource_reads() -> Vec<ResourceId> {
                let mut res = vec![];
                $(
                    res.append(&mut $ty::resource_reads());
                )*
                res
            }

            fn resource_writes() -> Vec<ResourceId> {
                let mut res = vec![];
                $(
                    res.append(&mut $ty::resource_writes());
                )*
                res
            }

            fn component_reads() -> Vec<ComponentTypeId> {
                let mut res = vec![];
                $(
                    res.append(&mut $ty::component_reads());
                )*
                res
            }

            fn component_writes() -> Vec<ComponentTypeId> {
                let mut res = vec![];
                $(
                    res.append(&mut $ty::component_writes());
                )*
                res
            }

            unsafe fn load_from_resources(resources: &mut Resources, ctx: SystemCtx, world: &World) -> Self {
                ($($ty::load_from_resources(resources, ctx.clone(), world) ,)*)
            }

            fn after_execution(&mut self) {
                $(self.$idx.after_execution() ;)*
            }
        }
    }
}

macro_rules! impl_data_output {
    ( $($ty:ident),* ) => {
        impl <'a, $($ty),*> SystemDataOutput<'a> for ($($ty ,)*) where $($ty: SystemDataOutput<'a>, )* {
            type SystemData = ($($ty::SystemData ,)*);
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

impl_data_output!(A);
impl_data_output!(A, B);
impl_data_output!(A, B, C);
impl_data_output!(A, B, C, D);
impl_data_output!(A, B, C, D, E);
impl_data_output!(A, B, C, D, E, F);
impl_data_output!(A, B, C, D, E, F, G);
impl_data_output!(A, B, C, D, E, F, G, H);
impl_data_output!(A, B, C, D, E, F, G, H, I);
impl_data_output!(A, B, C, D, E, F, G, H, I, J);
impl_data_output!(A, B, C, D, E, F, G, H, I, J, K);
impl_data_output!(A, B, C, D, E, F, G, H, I, J, K, L);
impl_data_output!(A, B, C, D, E, F, G, H, I, J, K, L, M);
impl_data_output!(A, B, C, D, E, F, G, H, I, J, K, L, M, N);
impl_data_output!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O);
impl_data_output!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);
impl_data_output!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q);
impl_data_output!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R);
impl_data_output!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S);
impl_data_output!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T);
impl_data_output!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U);
impl_data_output!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V);
impl_data_output!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W);
impl_data_output!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X);
impl_data_output!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y);
