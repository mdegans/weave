use eframe::epaint::ahash::HashMap;
use serde::{Deserialize, Serialize};

use crate::node::Node;

#[derive(Default, Serialize, Deserialize)]
pub struct Story {
    active: Option<Vec<usize>>,
    title: String,
    author_to_id: HashMap<String, u8>,
    id_to_author: Vec<String>,
    root: Node,
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
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_title(title: String) -> Self {
        Self {
            title,
            ..Self::default()
        }
    }

    /// Get the head node of the story. `head` is like git's `HEAD` and
    /// represents the current node the story is at.
    pub fn head(&self) -> &Node {
        match &self.active {
            Some(path) => self.root.iter_path_nodes(path).last().unwrap(),
            None => &self.root,
        }
    }

    /// Get mutable head node of the story.
    pub fn head_mut(&mut self) -> &mut Node {
        match &self.active {
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
        let author_id = self.get_author(author).unwrap();
        let head = self.head_mut();
        let child_index = head.add_child(Node::default());
        head.children[child_index].extend_strings(author_id, strings);
        if let Some(path) = &mut self.active {
            path.push(child_index);
        } else {
            self.active = Some(vec![child_index]);
        }
    }

    /// Extend the current paragraph with strings.
    pub fn extend_paragraph<Id>(
        &mut self,
        author: Id,
        strings: impl IntoIterator<Item = impl Into<String>>,
    ) where
        Id: Into<AuthorID>,
    {
        let author_id = match author.into() {
            AuthorID::String(author) => self.add_author(&author),
            AuthorID::ID(id) => {
                assert!(id < self.id_to_author.len() as u8, "Invalid author id");
                id
            }
        };
        self.head_mut().extend_strings(author_id, strings);
    }
}

impl std::fmt::Display for Story {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "# {}\n\nBy:", self.title)?;
        for (_, author) in self.authors() {
            writeln!(f, "- {}", author)?;
        }
        match &self.active {
            Some(path) => {
                for s in self.root.iter_path_text(&path, "\n") {
                    write!(f, "{}", s)?;
                }
            }
            None => {
                for s in self.root.iter_text() {
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
        let id = story.add_author("Bob");
        assert_eq!(Some(1), story.get_author("Bob"));
        story.add_paragraph(1, ["Goodbye", " World"]);
        story.extend_paragraph(id, ["!"]);
        assert_eq!(
            story.to_string(),
            "# Test\n\nBy:\n- Alice\n- Bob\n\nHello World\nGoodbye World!"
        );
        ["Alice", "Bob"]
            .iter()
            .enumerate()
            .for_each(|(id, &author)| {
                assert_eq!(Some(id as u8), story.get_author(author));
            });
    }
}
