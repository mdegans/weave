use std::{collections::HashMap, fmt::write};

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
    pub fn with_title(title: String) -> Self {
        Self {
            title,
            ..Self::default()
        }
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
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        ui.label(self.to_string());

        // Draw, and update active path if changed.
        if let Some(new_path) = self
            .root
            .draw(ui, self.active_path.as_ref().map(|v| v.as_slice()))
        {
            self.active_path = Some(new_path);
        }
    }
}

impl std::fmt::Display for Story {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "# {}", self.title)?;
        if self.author_to_id.is_empty() {
            writeln!(f, "By: Anonymous")?;
        } else {
            writeln!(f, "By:")?;
            for (_, author) in self.authors() {
                writeln!(f, "- {}", author)?;
            }
        }

        write!(f, "\n")?;

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_story() {
        let mut story = Story::with_title("Test".to_string());
        story.add_author("Alice");
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
