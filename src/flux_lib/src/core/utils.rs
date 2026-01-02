pub trait Ignore {
    fn ignore(self);
}

impl<T> Ignore for Option<T> {
    fn ignore(self) {  }
}

impl<R, E> Ignore for Result<R, E> {
    fn ignore(self) {  }
}