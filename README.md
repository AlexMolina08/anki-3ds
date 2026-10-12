
# Anki 3DS

An experimental port of [Anki](https://github.com/ankitects/anki) for the Nintendo 3DS family of handheld consoles.

The project aims to bring Anki's spaced repetition system to Nintendo 3DS hardware while reusing as much of the original codebase as possible.

## Project Status

> [!IMPORTANT]
> **Early development — feasibility stage.**
>
> This project is experimental. No functional release is currently available.

Development is initially focused on evaluating whether Anki's Rust backend (`rslib`) can be adapted to run on Nintendo 3DS hardware.

The original Nintendo 3DS is the primary target, with compatibility across the wider 3DS family as a goal.

## Goals

- **Native execution:** Run directly on Nintendo 3DS hardware without emulation.
- **Original Anki backend:** Reuse Anki's existing Rust implementation rather than developing a separate flashcard engine.
- **Offline reviews:** Study flashcards without an internet connection.
- **Collection compatibility:** Preserve compatibility with Anki's collection format and scheduling system.
- **AnkiWeb synchronization:** Investigate direct synchronization without requiring a computer or intermediary server.
- **Dual-screen interface:** Design an interface suited to the Nintendo 3DS hardware and touchscreen.

## Technical Approach

Anki's backend is primarily written in Rust. Nintendo 3DS homebrew development also supports Rust through the experimental `armv6k-nintendo-3ds` target.

The initial work will investigate:

1. Cross-compiling Anki's Rust backend for Nintendo 3DS.
2. Resolving platform-specific dependencies and system limitations.
3. Accessing and modifying Anki collections on local storage.
4. Running the original scheduling engine on the console.
5. Evaluating networking and direct AnkiWeb synchronization.

The feasibility of these components has not yet been established.

## Development

The project is currently in its research and prototyping phase.

Build instructions, development documentation, and technical findings will be added as implementation progresses.

## Upstream & Attribution

Anki 3DS is an unofficial fork of [Anki](https://github.com/ankitects/anki), originally developed by Damien Elmes and its contributors.

This project is not affiliated with or endorsed by the official Anki development team or Nintendo.

All applicable copyright notices and licensing requirements from the upstream project are retained.

## License

This project is based on Anki and is distributed under the applicable licenses of the original codebase.

See the repository's existing license files for details.
