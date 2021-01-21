use crate::renderer::list::{RenderList, RenderListCreationRecorder, RenderListRecorder};

pub trait UnsynchronizedRenderList: Send {
    fn init(&mut self, recorder: &mut RenderListCreationRecorder<'_>);
    fn render(&mut self, recorder: &mut RenderListRecorder<'_, '_>);
}

impl<T: UnsynchronizedRenderList> RenderList for std::sync::Mutex<T> {
    fn init(&self, recorder: &mut RenderListCreationRecorder<'_>) {
        self.lock().unwrap().init(recorder)
    }

    fn render(&self, recorder: &mut RenderListRecorder<'_, '_>) {
        self.lock().unwrap().render(recorder)
    }
}

impl<T: UnsynchronizedRenderList> RenderList for parking_lot::Mutex<T> {
    fn init(&self, recorder: &mut RenderListCreationRecorder<'_>) {
        self.lock().init(recorder)
    }

    fn render(&self, recorder: &mut RenderListRecorder<'_, '_>) {
        self.lock().render(recorder)
    }
}

impl<T: UnsynchronizedRenderList + Sync> RenderList for std::sync::RwLock<T> {
    fn init(&self, recorder: &mut RenderListCreationRecorder<'_>) {
        self.write().unwrap().init(recorder)
    }

    fn render(&self, recorder: &mut RenderListRecorder<'_, '_>) {
        self.write().unwrap().render(recorder)
    }
}

impl<T: UnsynchronizedRenderList + Sync> RenderList for parking_lot::RwLock<T> {
    fn init(&self, recorder: &mut RenderListCreationRecorder<'_>) {
        self.write().init(recorder)
    }

    fn render(&self, recorder: &mut RenderListRecorder<'_, '_>) {
        self.write().render(recorder)
    }
}
