The configuration file is `~/.config/kseqi/kseqi.conf`,
consisting of sequences and the associated action of each.
A line looks like:

```
R N N R = text "return" # Comment: Hold key R, Tap key N to type the word "return"
```

## Sequences

For maximal flexibility,
each line of configuration matches exactly one sequence of events.

For example, one sequence may involve:
Press left `Ctrl`, Press `X`, Release `X`, Release left `Ctrl`.
This is what one normally does when using a classic shortcut like `Ctrl+X`.

It can be written as:

```
Control_L X X Control_L
```

Including every event in a sequence makes it possible to have another shortcut
that begins with pressing `X`, followed by pressing left `Ctrl`.

Another example: make the space bar more useful with a sequence like:

```
space X X space
```

A sequence that will be familiar to Emacs users:

```
Control_L X X S S Control_L
```

that is, while holding the left `Ctrl` key, tap letter `X` and then `S`,
usually used to save a file.
But this sequence only triggers if the `Ctrl` is pressed the whole time,
so when used as a global shortcut, it does not interfere with `Ctrl+X`.

Names of keys follows the convention of X11,
such as `Alt_L`, `Shift_L`, `Super_L`.
They are what gets printed when you use a xorg util like `xev`.
Letters, as well as numbers are written literally, ignoring cases.

## Actions

To configure what to do when a sequence is recognized,
write the associated actions after the sequence and `=`.

A sequence can
- Launch a program or run a command

  Write `exec` followed by the executable and args, separated by space. An arg can optionally enclosed in quotes.
  
  Example: `exec xfce4-terminal --execute "htop"`
- Simulate keyboard input
  
  Input events are simulated by dynamically calling Xlib, making it more efficient than running an additional tool.
  
  Write `key` followed by the name of the key, when multiple keys need to be combined, use `+`.

  Example: `key Control_L+t`
- Type Unicode strings
  
  Write `text` followed by the content, such as `text "Hello, 世界 ω"`
- Simulate a mouse click
  
  Example: `mouse 1` generates a click of mouse button 1, or a left click.
- Perform a series of actions
  
  Separate with commas.
  
  Example: `key Escape, text ":wq", key enter` can be used to quit Vim.
- Repeat the most recent action
  
  If you've used Kseqi to type a word or move the cursor, invoking a shortcut associated with the action `repeat 3` would
  do it three more times. Kind of like how Vim and Emacs allows you to prefix an action with a number. But you
  don't need to decide beforehand.
