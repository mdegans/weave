# `weave` multiversal writing tool

Weave is a "multiversal" generative tree writing tool akin to [`loom`](https://github.com/socketteer/loom). It supports multiple generative backends such as:

- [x] [`drama_llama`](https://github.com/mdegans/drama_llama) - llama.cpp wrapper supporting all llama.cpp models
- [ ] OpenAI models
  - [ ] GPT 3.5 completions
  - [ ] Shim for GPT 4+ chat completions API.
- [ ] Anthropic models

# Features

The goal of `weave` is feature parity with [`loom`](https://github.com/socketteer/loom?tab=readme-ov-file#features).

- ☑️ Read mode
  - ✅ Linear story view
  - 🔲 Tree nav bar
  - 🔲 Edit mode
- ☑️ Tree view
  - ✅ Explore tree visually with mouse
  - ✅ Expand and collapse nodes
  - 🔲 Change tree topology
  - ✅ Edit nodes in place
- 🔲 Navigation
  - 🔲 Hotkeys
  - 🔲 Bookmarks
  - 🔲 Chapters
  - 🔲 'Visited' state
- ☑️ Generation
  - 🔲 Generate N children with various models (currently one a time).
  - ✅ Modify generation settings
- ☑️ File I/O
  - ✅ Serializable application state, including stories, to JSON.
  - 🔲 Open/save trees as JSON files
  - 🔲 Work with trees in multiple tabs
  - 🔲 Combine multiple trees
