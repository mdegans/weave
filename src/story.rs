use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::node::{Meta, Node};

#[derive(Default, Serialize, Deserialize)]
pub struct Story {
    active_path: Option<Vec<usize>>,
    pub title: String,
    author_to_id: HashMap<String, u8>,
    id_to_author: Vec<String>,
    root: Node<Meta>,
}

#[derive(derive_more::From)]
pub enum AuthorID {
    String(String),
    ID(u8),
}

impl From<&str> for AuthorID {
    fn from(author: &str) -> Self {
        Self::String(author.to_string())
    }
}

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

    /// Extend the current paragraph with strings.
    pub fn extend_paragraph(
        &mut self,
        strings: impl IntoIterator<Item = impl Into<String>>,
    ) {
        self.head_mut().extend_strings(strings);
    }

    /// Draw UI for the story.
    #[cfg(feature = "gui")]
    pub fn draw(&mut self, ui: &mut egui::Ui) -> Option<crate::node::Action> {
        use crate::node::PathAction;

        ui.label(self.to_string());

        // Draw, and update active path if changed.
        if let Some(PathAction { path, action }) = self
            .root
            .draw(ui, self.active_path.as_ref().map(|v| v.as_slice()))
        {
            self.active_path = Some(path);
            // FIXME: as it turns out all the actions are mutually exclusive,
            // so we can probably use an enum rather than a struct. The user can
            // only do one thing at a time, barring the UI hanging or something.
            if action.delete {
                // We can handle this here.
                self.decapitate();
                return None;
            } else if action.generate.is_some() | action.continue_ {
                return Some(action);
            }
        }

        None
    }

    /// Remove the head as well as all its children.
    pub fn decapitate(&mut self) {
        if let Some(path) = &mut self.active_path {
            if path.is_empty() {
                self.active_path = None;
            } else {
                let head_index = path.pop().unwrap();
                let mut node = &mut self.root;
                for i in path {
                    node = &mut node.children[*i];
                }
                // This wil now be the parent of the head node. We remove the
                // child index we just popped.
                node.children.remove(head_index);
            }
        }
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
}
