pub trait TryDefault {
    fn try_default() -> Option<Self>
    where
        Self: Sized;
}

impl<T> TryDefault for T {
    default fn try_default() -> Option<Self> {
        None
    }
}

impl<T> TryDefault for T
where
    T: Default,
{
    fn try_default() -> Option<Self> {
        Some(Self::default())
    }
}
