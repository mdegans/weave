use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::node::{Meta, Node};

#[derive(derive_more::From)]
pub enum AuthorID {
    String(String),
    ID(u8),
}

#[cfg(feature = "gui")]
pub enum DrawMode {
    /// Draw story as nodes, as usual.
    Nodes,
    /// Draw story as a collapsible tree.
    Tree,
}

impl From<&str> for AuthorID {
    fn from(author: &str) -> Self {
        Self::String(author.to_string())
    }
}

static_assertions::assert_impl_all!(AuthorID: Send, Sync);

#[derive(Default, Serialize, Deserialize)]
pub struct Story {
    active_path: Option<Vec<usize>>,
    pub title: String,
    author_to_id: HashMap<String, u8>,
    id_to_author: Vec<String>,
    root: Node<Meta>,
}

static_assertions::assert_impl_all!(Story: Send, Sync);

impl Story {
    pub fn new(title: String, author: String) -> Self {
        let mut new = Self {
            title,
            ..Self::default()
        };

        new.add_author(author);

        new
    }

    /// Get the head node of the story. `head` is like git's `HEAD` and
    /// represents the current node the story is at.
    pub fn head(&self) -> &Node<Meta> {
        match &self.active_path {
            Some(path) => self.root.iter_path_nodes(path).last().unwrap(),
            None => &self.root,
        }
    }

    /// Get mutable head node of the story.
    pub fn head_mut(&mut self) -> &mut Node<Meta> {
        match &self.active_path {
            Some(path) => {
                let mut node = &mut self.root;
                for &i in path {
                    node = &mut node.children[i];
                }
                node
            }
            None => &mut self.root,
        }
    }

    /// Add an author to the story. If the author already exists, return their
    /// id.
    pub fn add_author(&mut self, author: impl Into<String>) -> u8 {
        let author: String = author.into();
        if let Some(&id) = self.author_to_id.get(&author) {
            id
        } else {
            let new_id = self.id_to_author.len() as u8;
            self.id_to_author.push(author.clone());
            self.author_to_id.insert(author, new_id);
            new_id
        }
    }

    /// Get id for an author. If the author doesn't exist, return None.
    pub fn get_author<Id>(&self, author: Id) -> Option<u8>
    where
        Id: Into<AuthorID>,
    {
        match author.into() {
            AuthorID::String(author) => self.author_to_id.get(&author).copied(),
            AuthorID::ID(id) => {
                if id < self.id_to_author.len() as u8 {
                    Some(id)
                } else {
                    None
                }
            }
        }
    }

    /// Iterate over author ids and names.
    pub fn authors(&self) -> impl Iterator<Item = (u8, &str)> {
        self.id_to_author
            .iter()
            .enumerate()
            .map(|(id, author)| (id as u8, author.as_str()))
    }

    /// Add a node to the story's head node.
    pub fn paste_node(&mut self, mut node: Node<Meta>) {
        // We do this for now to avoid a crash. We can't transfer author ids
        // between stories yet, so we reset them to the head's author.
        node.set_author(self.head().author_id);
        self.head_mut().add_child(node);
    }

    /// Add paragraph to the story's head node.
    ///
    /// # Panics
    /// - If the author doesn't exist.
    pub fn add_paragraph<Id>(
        &mut self,
        author: Id,
        strings: impl IntoIterator<Item = impl Into<String>>,
    ) where
        Id: Into<AuthorID>,
    {
        let author = self.get_author(author).unwrap();
        let head = self.head_mut();
        let child_index = head.add_child(Node::with_author(author));
        let head = &mut head.children[child_index];
        head.extend_strings(strings);
        if let Some(path) = &mut self.active_path {
            path.push(child_index);
        } else {
            self.active_path = Some(vec![child_index]);
        }
    }

    /// Add an empty paragraph to the story's head node.
    pub fn add_empty_paragraph(&mut self, author: impl Into<AuthorID>) {
        const EMPTY: std::iter::Empty<String> = std::iter::empty();
        self.add_paragraph(author, EMPTY);
    }

    /// Extend the current paragraph with strings.
    pub fn extend_paragraph(
        &mut self,
        strings: impl IntoIterator<Item = impl Into<String>>,
    ) {
        self.head_mut().extend_strings(strings);
    }

    /// Draw UI for the story.
    ///
    /// If `lock_topology` is true, the user cannot add or remove nodes.
    #[cfg(feature = "gui")]
    pub fn draw(
        &mut self,
        ui: &mut egui::Ui,
        lock_topology: bool,
        layout: crate::node::Layout,
        mode: DrawMode,
        time_step: f32,
    ) -> Option<crate::node::Action> {
        use crate::node::PathAction;

        let selected_path = self.active_path.as_ref().map(|v| v.as_slice());

        // Draw, and update active path if changed.
        if let Some(PathAction { path, mut action }) = self.root.draw(
            ui,
            selected_path,
            lock_topology,
            layout,
            mode,
            time_step,
        ) {
            if !lock_topology {
                // Any action unless we're locked should update the active path.
                self.active_path = Some(path);
            }
            // FIXME: as it turns out all the actions are mutually exclusive,
            // so we can probably use an enum rather than a struct. The user can
            // only do one thing at a time, barring the UI hanging or something.
            if !lock_topology && action.delete {
                // We can handle this here.
                self.decapitate();
                action.modified = true;
                return None;
            } else if action.generate.is_some() | action.continue_ {
                return Some(action);
            }
        }

        None
    }

    /// Remove the head as well as all its children.
    ///
    /// Note: The root node is never removed.
    pub fn decapitate(&mut self) -> Option<Node<Meta>> {
        if let Some(path) = &mut self.active_path {
            if path.is_empty() {
                // There is always at least one node in the story.
                self.active_path = None;
            } else {
                let head_index = path.pop().unwrap();
                let mut node = &mut self.root;
                for i in path {
                    node = &mut node.children[*i];
                }
                // This will now be the parent of the head node. We remove the
                // child index we just popped.
                return Some(node.children.remove(head_index));
            }
        }

        return None;
    }

    /// Convert the story to a string with options
    pub fn format_full<F>(
        &self,
        mut f: F,
        include_authors: bool,
        include_title: bool,
    ) -> std::fmt::Result
    where
        F: std::fmt::Write,
    {
        if include_title {
            let title = if self.title.is_empty() {
                crate::consts::DEFAULT_TITLE
            } else {
                &self.title
            };
            writeln!(f, "# {}", title)?;
        }

        if include_authors {
            if self.author_to_id.is_empty() {
                writeln!(f, "By: {}", crate::consts::DEFAULT_AUTHOR)?;
            } else {
                writeln!(f, "By:")?;
                for (_, author) in self.authors() {
                    writeln!(f, "- {}", author)?;
                }
            }
        }

        if include_authors | include_title {
            write!(f, "\n")?;
        }

        match &self.active_path {
            Some(path) => {
                for s in self.root.iter_path_text(&path, "\n") {
                    write!(f, "{}", s)?;
                }
            }
            None => {
                for s in self.root.iter_pieces() {
                    write!(f, "{}", s)?;
                }
            }
        };

        Ok(())
    }

    /// Convert the story to OpenAI messages.
    #[cfg(feature = "openai")]
    pub fn to_openai_messages(&self) -> Vec<openai_rust::chat::Message> {
        use openai_rust::chat::Message;

        let messages = if let Some(path) = self.active_path.as_ref() {
            let mut messages: Vec<Message> = self
                .root
                .iter_path_nodes(path)
                .map(|node| Message {
                    role: self.id_to_author[node.author_id as usize].clone(),
                    content: node.to_string(),
                })
                .collect();

            // The last message is always the user's message. So we're going to
            // iterate in reverse and alternate between user and AI.
            // TODO: We can tag authors as user or assistant and use that
            // instead, but the messages won't alternate. That isn't strictly
            // necessary anymore, but it's what we specify in the default system
            // prompt. We can change that if we want, but it's something to be
            // done later.
            let mut is_user = true;
            for message in messages.iter_mut().rev() {
                message.role = if is_user {
                    "user".to_string()
                } else {
                    "assistant".to_string()
                };
                is_user = !is_user;
            }

            messages
        } else {
            // just the root node
            vec![Message {
                role: "user".to_string(),
                content: self.root.to_string(),
            }]
        };

        messages
    }
}

impl std::fmt::Display for Story {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.format_full(f, true, true)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn test_story() {
        let mut story = Story::new("Test".to_string(), "Alice".to_string());
        assert_eq!(Some(0), story.get_author("Alice"));
        story.add_paragraph("Alice", ["Hello", " World"]);
        story.add_author("Bob");
        assert_eq!(Some(1), story.get_author("Bob"));
        story.add_paragraph(1, ["Goodbye", " World"]);
        story.extend_paragraph(["!"]);
        assert_eq!(
            story.to_string(),
            "# Test\nBy:\n- Alice\n- Bob\n\n\nHello World\nGoodbye World!"
        );
        ["Alice", "Bob"]
            .iter()
            .enumerate()
            .for_each(|(id, &author)| {
                assert_eq!(Some(id as u8), story.get_author(author));
            });
    }

    // This tests we don't break backwards compatibility with the old format.
    #[test]
    fn test_story_deserialize() {
        let json_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test")
            .join("data")
            .join("sharks.0.0.3.json");

        assert!(json_path.exists(), "Test data not found: {:?}", json_path);

        let json = std::fs::read_to_string(json_path).unwrap();
        let story: Story = serde_json::from_str(&json).unwrap();
        assert_eq!(story.title, "Electrocuting Sharks");
        assert_eq!(story.root.count(), 23);
        let mut len = 0;
        for node in story.root.iter_depth_first() {
            len += node.text.len();
        }
        assert_eq!(len, 7453);
        // We're not checking the format or display because they may change.
    }
}
