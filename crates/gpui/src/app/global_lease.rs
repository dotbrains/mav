use super::*;

pub(crate) struct GlobalLease<G: Global> {
    pub(crate) global: Box<dyn Any>,
    global_type: PhantomData<G>,
}
impl<G: Global> GlobalLease<G> {
    pub(crate) fn new(global: Box<dyn Any>) -> Self {
        GlobalLease {
            global,
            global_type: PhantomData,
        }
    }
}

impl<G: Global> Deref for GlobalLease<G> {
    type Target = G;

    fn deref(&self) -> &Self::Target {
        self.global.downcast_ref().unwrap()
    }
}

impl<G: Global> DerefMut for GlobalLease<G> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.global.downcast_mut().unwrap()
    }
}
