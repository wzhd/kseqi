## Hotkey daemon with high degree of freedom

[![CI](https://github.com/wzhd/kseqi/actions/workflows/bui.yml/badge.svg)](https://github.com/wzhd/kseqi/actions)

This program doesn't limit you to the
typical keyboard combinations, which requires
modifier(s) plus one letter key, like `Ctrl` and `X`.

You can also combine keyboard events in myriad ways you might have considered.
Including shortcuts with:

- Only characters, no modifiers

  For example, a combination of `G` plus `O`.
The first letter, `G` here, works like a modifier,
pressing it can be easier on fingers than reaching for `Ctrl` or `Alt`.
Similarly, make the space bar more useful
by combining it with alphanumeric keys.

- Only modifier(s)

  Normally nothing happens when a key like `Ctrl` is tapped on its own.
Make better use of it by configuring an action.
And the left and right `Ctrl` can be used for different actions.

- One modifier, multiple characters

  Some existing software has integrated this kind of shortcuts.
For example, when using Emacs, one shortcut involves holding down `Ctrl`, pressing `X` and then `S`.
They also work as global shortcuts,
without interfering with existing shortcuts like `Ctrl`+`X`.


## Installation

Available as binary releases for Linux (X11) users:
[Releases](https://github.com/wzhd/kseqi/releases)

Alternatively, build from source with [cargo](https://rustup.rs/),
compilation takes 25s on Chromebook 2013.

Run the binary to try it out.
Or customize the [configuration file](https://github.com/wzhd/kseqi/blob/main/configuration.md).
