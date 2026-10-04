use crate::{
    app::FileManager,
    domain::location::Location,
    infrastructure::workspaces::Edit,
    state::actions::{Dialog, NameAction},
    ui::components::input::NameInput,
};
use gpui::{AppContext, Context, Window};
use std::path::PathBuf;

impl FileManager {
    pub(crate) fn workspace_dialog(
        &mut self,
        folder: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || self.dialog.is_some() || self.global_search.is_some() {
            return;
        }
        self.menu = None;
        self.error = None;
        let input = cx.new(|cx| NameInput::new(String::new(), window, cx));
        self.dialog = Some(Dialog::Name {
            action: NameAction::Workspace {
                folder: folder.clone(),
                names: Vec::new(),
            },
            input: input.clone(),
        });
        if folder.is_some() {
            let data = self.data_home.clone();
            let read = cx
                .background_executor()
                .spawn(async move { crate::infrastructure::workspaces::read(&data) });
            cx.spawn(async move |view, cx| {
                let result = read.await;
                let _ = view.update(cx, |view, cx| {
                    if let Some(Dialog::Name {
                        action: NameAction::Workspace { names, .. },
                        input: current,
                    }) = &mut view.dialog
                        && *current == input
                    {
                        match result {
                            Ok(workspaces) => {
                                *names = workspaces
                                    .into_iter()
                                    .map(|workspace| workspace.name)
                                    .collect()
                            }
                            Err(error) => {
                                view.error = Some(format!(
                                    "{}: {error}",
                                    view.language.text("Cannot read workspaces")
                                ))
                            }
                        }
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    pub(crate) fn edit_workspace(&mut self, edit: Edit, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        let data = self.data_home.clone();
        let task = cx
            .background_executor()
            .spawn(async move { crate::infrastructure::workspaces::edit(&data, edit) });
        cx.spawn(async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |view, cx| {
                view.busy = false;
                match result {
                    Ok(()) => view.navigate_location(Location::Workspaces, cx),
                    Err(error) => {
                        view.error = Some(format!(
                            "{}: {error}",
                            view.language.text("Cannot save workspace")
                        ))
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
