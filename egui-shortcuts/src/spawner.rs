use std::{
    future::Future,
    marker::PhantomData,
    sync::{Arc, Mutex},
};

use egui::Ui;
use poll_promise::Promise;

use crate::spawn_promise;

pub struct SimpleSpawner<T> {
    id: egui::Id,
    _phantom: PhantomData<T>,
}

type Container<T> = Option<Arc<Mutex<Promise<T>>>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnerState {
    /// We are waiting for spawn() to be called
    Waiting,
    /// The spawn event is still running
    Loading,
    /// The spawned task has returned and the spawner is displaying the result
    Done,
}

impl<T: Send + 'static> SimpleSpawner<T> {
    pub fn new(id: impl Into<egui::Id>) -> Self {
        Self {
            id: id.into(),
            _phantom: PhantomData,
        }
    }

    /// Spawns the task **only once**, requesting repaint on finish. Saves to temporary memory.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn once<F>(&self, ui: &mut Ui, f: F) 
    where
        F: Future<Output = T> + Send + 'static,
    {
        if self.get_state(ui) != SpawnerState::Waiting {
            return;
        }
        let ctx = ui.ctx().clone();

        let id = self.id;
        ui.ctx().memory_mut(move |w| {
            w.data.insert_temp(
                id,
                Some(Arc::new(Mutex::new(spawn_promise(async move {
                    let ret = f.await;
                    ctx.request_repaint();
                    ret
                })))),
            );
        });
    }

    #[cfg(target_arch = "wasm32")]
    pub fn once<F>(&self, ui: &mut Ui, f: F)
    where
        F: Future<Output = T> + 'static,
        F::Output: Send,
    {
        if self.get_state(ui) != SpawnerState::Waiting {
            return;
        }
        let ctx = ui.ctx().clone();

        let id = self.id;
        ui.ctx().memory_mut(move |w| {
            w.data.insert_temp(
                id,
                Some(Arc::new(Mutex::new(spawn_promise(async move {
                    let ret = f.await;
                    ctx.request_repaint();
                    ret
                })))),
            );
        });
    }




    pub fn get_state(&self, ui: &mut Ui) -> SpawnerState {
        let val = ui
            .ctx()
            .memory_mut(|w| w.data.get_temp::<Container<T>>(self.id).clone());

        match val {
            None => SpawnerState::Waiting,
            Some(None) => SpawnerState::Waiting,
            Some(Some(state)) => {
                if state.lock().unwrap().ready().is_some() {
                    SpawnerState::Done
                } else {
                    SpawnerState::Loading
                }
            }
        }
    }

    /// Spawns the task, requesting repaint on finish. Saves to temporary memory.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn<F>(&self, ui: &mut Ui, f: F)
    where
        F: Future<Output = T> + Send + 'static,
    {
        let ctx = ui.ctx().clone();

        let id = self.id;
        ui.ctx().memory_mut(move |w| {
            w.data.insert_temp(
                id,
                Some(Arc::new(Mutex::new(spawn_promise(async move {
                    let ret = f.await;
                    ctx.request_repaint();
                    ret
                })))),
            );
        });
    }

    #[cfg(target_arch = "wasm32")]
    pub fn spawn<F>(&self, ui: &mut Ui, f: F)
    where
        F: Future<Output = T> + 'static,
        F::Output: Send,
    {
        let ctx = ui.ctx().clone();

        let id = self.id;
        ui.ctx().memory_mut(move |w| {
            w.data.insert_temp(
                id,
                Some(Arc::new(Mutex::new(spawn_promise(async move {
                    let ret = f.await;
                    ctx.request_repaint();
                    ret
                })))),
            );
        });
    }

    pub fn reset(&self, ui: &mut Ui) {
        let id = self.id;
        ui.ctx()
            .memory_mut(move |w| w.data.remove::<Container<T>>(id));
    }

    pub fn show(&self, ui: &mut Ui, f: impl FnOnce(&mut Ui, &mut T)) {
        if ui
            .ctx()
            .memory(|w| w.data.get_temp::<Container<T>>(self.id))
            .is_none()
        {
            ui.label("Value not set.");
        } else {
            let val = ui.ctx().memory_mut(|w| {
                w.data
                    .get_temp_mut_or_default::<Container<T>>(self.id)
                    .clone()
                    .unwrap()
            });

            let mut lck = val.lock().unwrap();

            if let Some(ready) = lck.ready_mut() {
                f(ui, ready)
            } else {
                ui.label("Working ...");
            }
        }
    }
}
