use std::io::Cursor;
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Result};
use clap::Parser;
use fs_err as fs;
use tracing::*;
use uesave::{Property, PropertyType, Save, StructType, StructValue, ValueArray};
use uuid::Uuid;

#[derive(Debug, Parser)]
struct Args {
    /// Path to the save file that you want to edit.
    path: PathBuf,
}

fn main() -> Result<()> {
    logging::setup_logging();
    let Args { path } = Args::parse();

    edit_save(&path)?;
    Ok(())
}

fn edit_save(save_path: &Path) -> Result<()> {
    info!("editing save file: `{}`", save_path.display());
    let backup_save_path = save_path.with_extension("sav.bak");
    fs::copy(&save_path, &backup_save_path)?;
    info!("creating backup save file: `{}`", backup_save_path.display());

    let mut buf = fs::read(save_path)?;
    let mut cursor = Cursor::new(&mut buf);
    let mut save = Save::read(&mut cursor)?;

    let Property::Array { array_type, ref mut value, .. } =
        &mut save.root.properties["CharacterSaves"]
    else {
        bail!(r#"expected `Property::Array` for `root.properties["CharacterSaves"]`"#);
    };
    ensure!(*array_type == PropertyType::StructProperty, "unexpected property type");

    // Single struct property inside the array
    let ValueArray::Struct { _type, name, struct_type, value: ref mut class_saves, .. } = value
    else {
        bail!("unexpected length for character saves array");
    };
    ensure!(_type == "CharacterSaves", "unexpected value array `_type`");
    ensure!(name == "StructProperty");
    ensure!(*struct_type == StructType::Struct(Some("CharacterSave".to_string())));

    // There are 5 class save slots. 4 of them are active classes, but 1 of them is unused (but
    // still contributes to blue number). This means we can manipulate overall blue number by
    // manipulating hidden class's `RetiredCharacterLevels`.
    ensure!(class_saves.len() == 5, "expected 5 class save slots");

    let (mut inactive_class_saves, mut active_class_saves): (Vec<_>, Vec<_>) =
        class_saves.iter_mut().partition(|class_save| {
            let StructValue::Struct(ref props) = class_save else {
                panic!("unexpected `class_save` struct value kind");
            };

            let inactive_class_save_uuid: Uuid =
                uuid::uuid!("d6d5686e-4547-e66f-46c5-ce8e28b16827");

            let Property::Struct {
                value: StructValue::Guid(found_uuid),
                struct_type: StructType::Guid,
                ..
            } = props["SavegameID"]
            else {
                return false;
            };

            inactive_class_save_uuid == found_uuid
        });
    ensure!(inactive_class_saves.len() == 1, "expected exactly 1 inactive class");

    const MIN_PROMOS: i32 = 1;
    const RED_LEVELS_PER_PROMO: i32 = 25;

    for class_save in active_class_saves.iter_mut() {
        let StructValue::Struct(ref mut props) = class_save else {
            bail!("unexpected `class_save` struct value kind");
        };

        debug!(times_retired_before = ?props["TimesRetired"]);
        debug!(retired_character_levels_before = ?props["RetiredCharacterLevels"]);

        // There are two properties important here:
        // 1. `TimesRetired`: I think this is how many promos you have for a class
        // 2. `RetiredCharacterLevels`: I think this is how many red levels you have for a class?
        // I think that blue level is calculated by something like the total of `25 * times_retired`
        // (25 red levels needed per promo) + `retired_character_levels` (residual) per class,
        // summed together for the whole save, and then divided by 3 floored.
        {
            let Property::Int { value, .. } = &mut props["TimesRetired"] else {
                bail!("`TimesRetired` not found");
            };
            // Set to at least 1 promo so we can still play deep dives.
            *value = MIN_PROMOS;
        }
        {
            let Property::Int { value, .. } = &mut props["RetiredCharacterLevels"] else {
                bail!("`RetiredCharacterLevels` not found");
            };
            // Set to 25 red levels to lock red levels (unless promote).
            *value = RED_LEVELS_PER_PROMO;
        }

        debug!(times_retired_after = ?props["TimesRetired"]);
        debug!(retired_character_levels_after = ?props["RetiredCharacterLevels"]);
    }

    let active_red_levels = (active_class_saves.len() as i32) * (MIN_PROMOS * RED_LEVELS_PER_PROMO);
    let active_blue_level = active_red_levels / 3; // truncates, equivalent to flooring

    let target_blue_level = -69;
    let diff_blue_level = target_blue_level - active_blue_level;
    let diff_red_level = diff_blue_level * 3;

    // Use the inactive class to modify blue level, which does not show up for active classes.
    let inactive_class_save = inactive_class_saves.remove(0);
    {
        let StructValue::Struct(ref mut props) = inactive_class_save else {
            bail!("unexpected `class_save` struct value kind");
        };

        debug!(times_retired_before = ?props["TimesRetired"]);
        debug!(retired_character_levels_before = ?props["RetiredCharacterLevels"]);

        {
            let Property::Int { value, .. } = &mut props["TimesRetired"] else {
                bail!("`TimesRetired` not found");
            };
            // Unneeded, zeroed so do not affect blue level calculation.
            *value = 0;
        }
        {
            let Property::Int { value, .. } = &mut props["RetiredCharacterLevels"] else {
                bail!("`RetiredCharacterLevels` not found");
            };
            // Use this to influence the desired blue level. Blue level and red level can be
            // negative!
            *value = diff_red_level;
        }

        debug!(times_retired_after = ?props["TimesRetired"]);
        debug!(retired_character_levels_after = ?props["RetiredCharacterLevels"]);
    }

    save.write(&mut Cursor::new(&mut buf))?;
    fs::write(save_path, &buf)?;

    info!("replaced `{}` with modified save file", save_path.display());
    Ok(())
}
