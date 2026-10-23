use crate::{
    app::FileManager,
    domain::{location::Location, tag::Tag},
};
use gpui::Context;

impl FileManager {
    pub(crate) fn edit_tags(&mut self, tag: Option<Tag>, cx: &mut Context<Self>) {
        if self.busy
            || self.active_operation.is_some()
            || self.loading
            || self.location.remote().is_some()
            || self.location == Location::Trash
        {
            return;
        }
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        let enabled = tag.is_some_and(|tag| {
            !paths
                .iter()
                .all(|path| self.tags.colors(path).contains(&tag))
        });
        self.menu = None;
        self.busy = true;
        self.error = None;
        let data = self.data_home.clone();
        let task = cx
            .background_executor()
            .spawn(async move { crate::infrastructure::tags::set(&data, &paths, tag, enabled) });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |view, cx| {
                view.busy = false;
                match result {
                    Ok(tags) => {
                        view.tags = tags;
                        if matches!(view.location, Location::Tag(_)) {
                            view.refresh(cx);
                        }
                    }
                    Err(error) => {
                        view.error = Some(format!(
                            "{}: {error}",
                            view.language.text("Cannot save tags")
                        ))
                    }
                }
                view.start_queued_operation(false, cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
