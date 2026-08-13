//! `lure onboard` 创建的 agent workspace 默认模板。
//!
//! 放在 core 中作为单一事实来源：onboard 写入它们，ContextBuilder 也据此识别未定制的
//! AGENTS/USER 模板并跳过注入。

pub const DEFAULT_SOUL: &str = r#"# Soul

I am Lure, a personal AI assistant.

## Core Principles

- Solve by doing, not by describing what I would do.
- Keep responses short unless depth is asked for.
- Say what I know, flag what I don't, and never fake confidence.
- Stay friendly and curious; ask a good question when guessing would be risky.
- Treat the user's time as scarce and their trust as valuable.
"#;

pub const DEFAULT_USER: &str = r#"# User Profile

Information about the user to help personalize interactions.

## Basic Information

- **Name**: (your name)
- **Timezone**: (your timezone, e.g., UTC+8)
- **Language**: (preferred language)

## Preferences

### Communication Style

- [ ] Casual
- [ ] Professional
- [ ] Technical

### Response Length

- [ ] Brief and concise
- [ ] Detailed explanations
- [ ] Adaptive based on question

### Technical Level

- [ ] Beginner
- [ ] Intermediate
- [ ] Expert

## Work Context

- **Primary Role**: (your role, e.g., developer, researcher)
- **Main Projects**: (what you're working on)
- **Tools You Use**: (IDEs, languages, frameworks)

## Topics of Interest

-
-
-

## Special Instructions

(Any specific instructions for how the assistant should behave)
"#;

pub const DEFAULT_AGENTS: &str = r#"# Agent Instructions

## Workspace Guidance

Use this file for project-specific preferences, recurring workflow conventions, and instructions you want the agent to remember for this workspace. Keep durable facts about the user in `USER.md`, personality/style guidance in `SOUL.md`, and long-term memory in `memory/MEMORY.md`.

## Scheduled Reminders

- Before scheduling reminders, check available skills and follow skill guidance first.
- Use the built-in `cron` tool to create/list/remove jobs.
- Cron jobs run as scheduled turns in the origin chat/session and normally deliver the result back to that channel. Do not use cron for background checks that should stay silent when there is nothing useful to report; use `HEARTBEAT.md` instead.

## Heartbeat Tasks

`HEARTBEAT.md` is checked periodically by the protected heartbeat job. Do not create a duplicate heartbeat job unless the user has disabled the built-in one and explicitly wants a custom schedule.
"#;

pub const DEFAULT_HEARTBEAT: &str = r#"# Heartbeat Tasks

<!--
This file is checked periodically by your Lure agent.

Use this file for recurring background checks that should stay quiet unless there is something useful to report. Regular cron jobs are different: they normally deliver each run's result back to the chat/session where they were created.

If this file has no tasks (only headers and comments), the agent will skip it. Completed tasks should be deleted, not kept - heartbeat only reads "Active Tasks".
-->

## Active Tasks

<!-- Add your periodic tasks below this line -->
"#;
