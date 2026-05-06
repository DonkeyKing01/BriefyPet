# BriefyPet

<p align="center">
  <img src="src-tauri/icons/icon.png" alt="BriefyPet icon" width="112" />
</p>

<p align="center">
  <strong>A quiet desktop reading companion</strong>
</p>

<p align="center">
  <a href="https://opensource.org/licenses/MIT"><img alt="License: MIT" src="https://img.shields.io/badge/License-MIT-yellow.svg"></a>
  <img alt="Tauri" src="https://img.shields.io/badge/Tauri-1.8-24C8DB">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-2021-B7410E">
</p>

<p align="center">
  <a href="./README.md">中文</a> | <a href="./README.en.md">English</a>
</p>

## What is BriefyPet?

> Feeling overwhelmed by information?
>
> Tired of complex source-code deployment?
>
> Do your subscriptions keep piling up in email inboxes and RSS readers that you never open?

As AI-native students, we deeply feel both the excitement and anxiety brought by today's flood of information. BriefyPet started from a very Chinese-language information environment: many people want to stay close to frontier ideas, but the path from primary sources to everyday reading is often slowed down by language barriers, platform habits, fragmented channels, and second-hand reposts.

Existing readers and information subscription tools still leave several problems unsolved:

1. How to make information more reachable, instead of letting it remain buried in professional tools and email inboxes;
2. How to personalize information needs, instead of showing the same recommendations to users with very different interests;
3. How to lower the barrier for non-technical users and avoid complex source-code deployment or configuration workflows;
4. How to build long-term memory, so users can accumulate knowledge and gradually calibrate their interests.

BriefyPet is an intelligent desktop reading companion that tries to address these problems through source curation, personalization, memory, and reachability.

It is not a traditional RSS reader. Instead, it is an intelligent desktop assistant that combines collection, filtering, and reminders: it first controls the quality of primary sources, then summarizes articles, evaluates their relevance, and explains why they may be worth reading based on your interests. Finally, it only reminds you when something truly deserves priority attention.

## Preview

### Reading View

![BriefyPet reading view](docs/reading.png)

### Settings View

![BriefyPet settings view](docs/setting.png)


## Features

- A local desktop application that can be installed and run directly, without account registration or source-code installation;
- Information subscriptions are fetched through RSS, without web crawlers or complex API requests;
- The desktop pet only appears during task execution and reminders, balancing playfulness with low interruption and high reachability;
- Personalized interest configuration that keeps calibrating as you use the app;
- LLM-generated Chinese summaries, relevance scores, and recommendation reasons;
- Supports article reading, opening original links, favorites, notes, unread items, and reading history;
- Supports mainstream LLM providers and custom configuration import;
- Data is stored locally and does not depend on a cloud account.

## Quick Start

Visit [Release](https://github.com/DonkeyKing01/BriefyPet/releases) or the [product release page](https://briefypet.netlify.app/) to download the installer. After the first launch, you need to:

1. Choose an LLM provider;
2. Enter the API Key for the corresponding provider;
3. Select the modules and subcategories you want to follow;
4. Write down your interest preferences.

After configuration, BriefyPet will start fetching RSS feeds, calling the LLM for summarization and scoring, and reminding you when high-value content appears.

Common API Key entry points:

- [OpenAI API Key](https://platform.openai.com/api-keys)
- [Anthropic API Key](https://console.anthropic.com/settings/keys)
- [Gemini API Key](https://aistudio.google.com/app/apikey)
- [DeepSeek API Key](https://platform.deepseek.com/api_keys)
- [Qwen API Key](https://help.aliyun.com/zh/dashscope/opening-service)
- [MiniMax API Key](https://platform.minimax.io/docs/guides/quickstart)
- [GLM API Key](https://docs.bigmodel.cn/)
- [Kimi API Key](https://platform.moonshot.ai/console/api-keys)



## Sources

The current source catalog is located at `src-tauri/resources/rss-catalog.opml`. It covers 6 top-level domains, 24 secondary disciplines, 47 fine-grained categories, and 760+ subscription entries in total. It focuses on high-quality sources in technology, medicine, basic science, social science, design, and business.

| Top-level Domain | Example Subcategories | Typical Sources |
| --- | --- | --- |
| Technology & AI | AI & Computer Science, Embodied AI & Robotics, HCI Research | Official accounts, research teams, engineering blogs, and high-density creator viewpoints |
| Social Science | Economics, Sociology, Political Science | Frontiers in economics, sociology, political science, and serious commentary sources |
| Medicine | Clinical Medicine, Public Health, Drug Development, Biomedical Engineering | Signals from clinical medicine, public health, bioengineering, and drug development |
| Basic Science | Biology, Physics, Chemistry & Materials, Environmental Science, Mathematics | Sources from physics, chemistry, biology, and interdisciplinary frontiers |
| Design | Product & UX, Engineering Design, Creative Design, Design Methods | Product and UX design, HCI research, and engineering design |
| Business | Media Coverage, Industry Observation, Long-term Perspectives | Business media, industry observations, and long-term viewpoints |

Users can also add custom RSS sources in the settings page. Newly added sources will be assigned to the corresponding module and category, and will enter the subsequent fetching, summarization, and scoring workflow.

## IP Character

We chose the black-crowned night heron, jokingly known in Chinese internet culture as the “Chinese backyard penguin,” as our desktop pet character. It is quiet, alert, and thoughtful, silently filtering high-value information and protecting the user's precious attention.

We hope you will encounter the following night heron states while using BriefyPet:

| State | Meaning | Preview |
| --- | --- | --- |
| loading | The app is starting or loading | <img src="public/pets/briefy-ip/gifs/loading.gif" alt="loading" width="96" /><br>`public/pets/briefy-ip/gifs/loading.gif` |
| needs-config | Required configuration has not been completed | <img src="public/pets/briefy-ip/gifs/needs-config.gif" alt="needs-config" width="96" /><br>`public/pets/briefy-ip/gifs/needs-config.gif` |
| polling | Periodically polling sources | <img src="public/pets/briefy-ip/gifs/polling.gif" alt="polling" width="96" /><br>`public/pets/briefy-ip/gifs/polling.gif` |
| scanning | Fetching, summarizing, and filtering | <img src="public/pets/briefy-ip/gifs/scanning.gif" alt="scanning" width="96" /><br>`public/pets/briefy-ip/gifs/scanning.gif` |
| idle | No high-priority reminders at the moment | <img src="public/pets/briefy-ip/gifs/idle.gif" alt="idle" width="96" /><br>`public/pets/briefy-ip/gifs/idle.gif` |
| new-info | New content is worth reading | <img src="public/pets/briefy-ip/gifs/new-info.gif" alt="new-info" width="96" /><br>`public/pets/briefy-ip/gifs/new-info.gif` |



## Data & Privacy

BriefyPet does not require a cloud account. Articles, favorites, notes, reminder queues, interest memory, and runtime states are stored by default in a local SQLite database.

Please note that summarization, scoring, and memory extraction will call the LLM provider configured by the user. In other words, what gets sent to the model provider depends on the API and model service terms you choose. The project itself does not provide a cloud account system and does not sync your local database to any project server.

## Q&A

### Is BriefyPet a traditional RSS reader?

No. It is more like a combination of “source entry + LLM pre-filtering + desktop reminders + reading accumulation.” You do not need to keep opening an information feed manually. BriefyPet filters out low-quality noise and presents truly relevant content to you.

### Why emphasize high-quality primary sources?

Academic frontiers, official releases, and creator viewpoints are closer to the original signal. They usually have higher information density and long-term value, making them more suitable for knowledge accumulation than second-hand packaged trend consumption.

### Can I use it without an API Key?

The current version does not support a no-key mode. Without an available API Key, the desktop pet will remain in the needs-config state and cannot proceed to summarization, relevance evaluation, or recommendation.

## Build Locally

Requirements:

- Node.js 18+
- npm
- Rust stable
- System dependencies required by Tauri 1.x

Install dependencies:

```bash
npm install
```

Start development mode:

```bash
npm run tauri dev
```

Start frontend only:

```bash
npm run dev
```

Build frontend:

```bash
npm run build
```

macOS release DMG:

```bash
npm run tauri:build:mac:release-dmg
```

Windows debug bundle:

```powershell
npm run tauri:build:windows:debug
```

Windows release bundle:

```powershell
npm run tauri:build:windows:release
```

The packaged artifacts are located by default at:

```text
src-tauri/target/release/bundle
```

## Project Structure

```text
BriefyPet/
├── src/                         # React frontend
│   ├── App.tsx                   # Main window, desktop pet, bubbles, help, and memory confirmation window
│   ├── api.ts                    # Tauri command wrapper
│   ├── styles.css                # Frontend styles
│   └── types.ts                  # Shared frontend-backend type definitions
├── src-tauri/                    # Tauri/Rust backend
│   ├── src/
│   │   ├── main.rs               # App entry, windows, tray, and plugin initialization
│   │   ├── commands.rs           # Commands callable from the frontend
│   │   ├── db.rs                 # SQLite storage, migrations, snapshots, and source catalog
│   │   ├── rss.rs                # RSS/Atom fetching and parsing
│   │   ├── llm.rs                # LLM requests, summary scoring, and memory extraction
│   │   ├── service.rs            # Scheduling, state sync, reminders, and memory workflow
│   │   ├── policy.rs             # Modules, categories, fetch frequency, and scoring strategy
│   │   ├── tray.rs               # System tray
│   │   └── models.rs             # Rust data models
│   ├── resources/
│   │   └── rss-catalog.opml      # Built-in source catalog
│   ├── icons/                    # App icons
│   └── tauri.conf.json           # Tauri configuration
├── public/
│   └── pets/briefy-ip/           # Desktop pet main image and animated state GIFs
├── docs/                         # README screenshots
├── scripts/                      # macOS/Windows packaging scripts
├── package.json                  # Frontend dependencies and scripts
└── vite.config.ts                # Vite configuration
```

## Acknowledgements

Thanks to collaborator [Reed2006](https://github.com/Reed2006)

Thanks to [Journeylzx](https://github.com/Journeylzx) for inspiring the idea and supporting me all the way

And thanks to everyone who gave valuable feedback during development

We were also inspired by the following projects:

- [BestBlogs](https://github.com/ginobefun/BestBlogs)
- [clawd-on-desk](https://github.com/rullerzhou-afk/clawd-on-desk)

## License

MIT
