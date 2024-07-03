# Collection of save file experiments

**This repository is completely unsupported and untested. Assume bugs will not be fixed.**
Using tools in this repository **will brick a save**. Always make backups.

## Blue number resetter

Resets your DRG blue rank to `-69` while still being able to play EDDs.
This is achieved in part by setting each active class to 1 promo + 25 residual red levels.

```
$ cargo run -p blue-number-resetter -- <path_to_sav>
```
