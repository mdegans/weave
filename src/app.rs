use egui::Widget;

use crate::story::Story;

#[derive(Default)]
pub struct Toolbar {
    pub title_buf: String,
}

pub struct Viewport {
    pub scroll: egui::Vec2,
    pub zoom: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            scroll: Default::default(),
            zoom: 1.0,
        }
    }
}

#[derive(Default)]
pub enum SideBarState {
    #[default]
    Closed,
    Opening,
    Open,
    Closing,
}

#[derive(Default, derive_more::Display)]
pub enum SidebarPage {
    #[default]
    Stories,
    Settings,
}

#[derive(Default)]
pub struct Sidebar {
    state: SideBarState,
    page: SidebarPage,
}

#[derive(Default)]
pub struct App {
    active_story: Option<usize>,
    stories: Vec<Story>,
    sidebar: Sidebar,
    toolbar: Toolbar,
    viewport: Viewport,
}

impl App {
    pub fn new<'s>(cc: &eframe::CreationContext<'s>) -> Self {
        let stories = cc
            .storage
            .map(|storage| {
                storage
                    .get_string("stories")
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        Self {
            stories,
            active_story: None,
            ..Default::default()
        }
    }

    pub fn new_story(&mut self, title: String) {
        self.stories.push(Story::with_title(title));
        self.active_story = Some(self.stories.len() - 1);
    }

    /// (active) story
    pub fn story(&self) -> Option<&Story> {
        self.active_story.map(|i| &self.stories[i])
    }

    /// (active) story
    pub fn story_mut(&mut self) -> Option<&mut Story> {
        self.active_story.map(move |i| self.stories.get_mut(i))?
    }
}

impl eframe::App for App {
    fn update(
        &mut self,
        ctx: &eframe::egui::Context,
        frame: &mut eframe::Frame,
    ) {
        egui::TopBottomPanel::top("toolbar")
            .resizable(true)
            .show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    if ui.button("New Story").clicked() {
                        let title = if self.toolbar.title_buf.is_empty() {
                            "Untitled".to_string()
                        } else {
                            let title = self.toolbar.title_buf.clone();
                            self.toolbar.title_buf.clear();
                            title
                        };
                        self.new_story(title);
                    }
                    ui.text_edit_singleline(&mut self.toolbar.title_buf);
                });
            });

        egui::SidePanel::left("sidebar")
            .default_width(200.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading(self.sidebar.page.to_string());

                match self.sidebar.page {
                    SidebarPage::Settings => {}
                    SidebarPage::Stories => {
                        let mut delete = None;
                        for (i, story) in self.stories.iter().enumerate() {
                            ui.horizontal(|ui| {
                                if ui.button("X").clicked() {
                                    delete = Some(i);
                                }
                                if ui.button(&story.title).clicked() {
                                    self.active_story = Some(i);
                                }
                            });
                        }
                        if let Some(i) = delete {
                            self.stories.remove(i);
                            if self.active_story == Some(i) {
                                self.active_story = None;
                            }
                        }
                    }
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(story) = self.story_mut() {
                story.draw(ui);
            } else {
                ui.heading("Welcome to Weave!");
                ui.label("Create a new story or select an existing one.");
            }
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(
            "stories",
            serde_json::to_string(&self.stories).unwrap(),
        )
    }
}
