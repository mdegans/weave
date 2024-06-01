# `weave` multiversal writing tool

![Weave Icon](/resources/icon.inkscape.svg)

Weave is a "multiversal" generative tree writing tool akin to [`loom`](https://github.com/socketteer/loom). It supports multiple generative backends such as:

- ✅ [`drama_llama`](https://github.com/mdegans/drama_llama) - llama.cpp wrapper supporting all llama.cpp models
- ✅ OpenAI models
  - ✅ Shim for GPT 3.5+ chat completions API, including GPT 4o.
- 🔲 Anthropic models

## Features

Notable features:

- **Live switching of backends** - Generate part of a story
  with OpenAI and another part with LLaMA -- all without restarting the app.
- **Streaming responses** - Cancel generations in progress --
  both local and online.
- **Live editing** - Edit posts during generation. New tokens are always appended to the end.
- **Advanced sampling controls** - For local language models. Use any sampling methods in any order.

Coming soon:

- Keyboard shortcuts.

Additionally, one goal of `weave` is feature parity with [`loom`](https://github.com/socketteer/loom?tab=readme-ov-file#features).

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
  - ✅ Modify generation settings (Complete for OpenAI and mostly for local)
- ☑️ File I/O
  - ✅ Serializable application state, including stories, to JSON.
  - ✅ Open/save trees as JSON files
  - 🔲 Work with trees in multiple tabs
  - 🔲 Combine multiple trees

# Notable issues

- On some platforms (like MacOS) the Weave icon will change to an `e` shortly
  after launch. See [this
  issue](https://github.com/emilk/egui/issues/3823#issuecomment-1892423108) for
  details.
- With each new generation, all tokens need to be injested again with most
  backends. This is solvable with `drama_llama` (longest prefix cache) but not
  for the OpenAI API. So for OpenAI, it's recommended to generate larger posts.
  The system prompt is customizable so you can tweak the agent's instructions on
  verbosity.
- It is not currently possible to have a scrollable viewport so it's
  recommended to collapse nodes if things get cluttered. This is because the
  nodes are implemented with [`egui::containers::Window`](https://docs.rs/egui/latest/egui/containers/struct.Window.html) which ignore scrollable areas. This is fixable
  but not easily and not cleanly. When it is resolved the central panel will be
  split into story and node views.
