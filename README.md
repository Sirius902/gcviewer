# gcviewer
A customizable input viewer.

**Note:** As of right now gcviewer only works with [gcfeeder](https://github.com/Sirius902/gcfeeder)
but will be updated to support other controllers in the future.

## Q/A
### Why is it named gcviewer?

Originally this was a part of [gcfeeder](https://github.com/Sirius902/gcfeeder) but has moved to this repository after commit
[c4c65b2](https://github.com/Sirius902/gcviewer/commit/c4c65b291bec4ac31879d24497caa13c22acbe81). The name was made as combination
of GameCube and viewer, and, despite separating into a different repository the name stuck.

## Program Arguments
For info on the program arguments, run with `--help`.

## Themes
To customize the theme of the input viewer, put a `color.frag` according to the specification in the same directory as the executable.
The default theme can be found at `src/shader/color.frag`. Other themes can be found in `theme`.
