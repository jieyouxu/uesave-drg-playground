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

    for (i, class_save) in active_class_saves.iter_mut().enumerate() {
        const XP_REQUIRED_FOR_25_RED_LEVELS: i32 = 315_000;
        set_class_save(
            format!("active_class {i}"),
            class_save,
            1,
            25,
            XP_REQUIRED_FOR_25_RED_LEVELS,
        )?;
    }

    let n_active_classes = active_class_saves.len() as i32;
    let active_red_level = n_active_classes * (1 * 25 + 25);
    let target_blue_level = -69;
    let target_red_level = target_blue_level * 3;
    let diff_red_level = target_red_level - active_red_level;

    // Use the inactive class to modify blue level, which does not show up for active classes.
    set_class_save("inactive_class", inactive_class_saves.remove(0), 0, diff_red_level, 0)?;

    save.write(&mut Cursor::new(&mut buf))?;
    fs::write(save_path, &buf)?;

    info!("replaced `{}` with modified save file", save_path.display());
    Ok(())
}

#[instrument(level = "debug", skip(class_save), fields(save_name = save_name.as_ref()))]
fn set_class_save<S: AsRef<str>>(
    save_name: S,
    class_save: &mut StructValue,
    promos: i32,
    red_levels: i32,
    xp: i32,
) -> Result<()> {
    let StructValue::Struct(ref mut props) = class_save else {
        bail!("unexpected `class_save` struct value kind");
    };

    {
        let Property::Int { value, .. } = &mut props["TimesRetired"] else {
            bail!("`TimesRetired` not found");
        };
        // Unneeded, zeroed so do not affect blue level calculation.
        *value = promos;
    }
    {
        let Property::Int { value, .. } = &mut props["RetiredCharacterLevels"] else {
            bail!("`RetiredCharacterLevels` not found");
        };
        // Use this to influence the desired blue level. Blue level and red level can be
        // negative!
        *value = red_levels;
    }
    {
        let Property::Int { value, .. } = &mut props["XP"] else {
            bail!("`XP` not found");
        };
        *value = xp;
    }

    Ok(())
}
