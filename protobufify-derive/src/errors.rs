//! Errors collected to be reported together, so that one expansion reports every problem of a
//! type at once.

use syn::Error;

#[derive(Default)]
pub(crate) struct Errors(Option<Error>);

impl Errors {
    pub(crate) fn push(&mut self, error: Error) {
        match &mut self.0 {
            Some(errors) => errors.combine(error),
            None => self.0 = Some(error),
        }
    }

    pub(crate) fn finish(self) -> syn::Result<()> {
        self.0.map_or(Ok(()), Err)
    }
}
