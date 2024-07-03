use std::io::Cursor;
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Result};
use clap::Parser;
use fs_err as fs;
use tracing::*;
use uesave::{Property, PropertyType, Save, StructType, StructValue, ValueArray};

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

    // For whatever reason there are five classes here instead of the four. One of them seems
    // unused, but let's change all of them to be safe.
    ensure!(class_saves.len() >= 4, "expected at least 4 classes");

    for (i, class_save) in class_saves.iter_mut().enumerate() {
        debug!(i, "slot_id");
        let StructValue::Struct(ref mut props) = class_save else {
            bail!("unexpected `class_save` struct value kind")
        };

        debug!(times_retired_before = ?props["TimesRetired"]);
        debug!(retired_character_levels_before = ?props["RetiredCharacterLevels"]);

        // There are two properties important here:
        // 1. `TimesRetired`: I think this is how many promos you have for a class
        // 2. `RetiredCharacterLevels`: I think this is how many red levels you have for a class?
        // I think that blue level is calculated by something like the total of `25 * times_retired`
        // (25 red levels needed per promo) + `retired_character_levels` (residual) per class,
        // summed together for the whole save, and then divided by 3 floored.
        // So we try to set the minimum here: 1 promo for each class (to ensure we can still play
        // deep dives) and 1 residual red level just to be safe.
        {
            let Property::Int { value, .. } = &mut props["TimesRetired"] else {
                bail!("`TimesRetired` not found");
            };
            *value = 1;
        }
        {
            let Property::Int { value, .. } = &mut props["RetiredCharacterLevels"] else {
                bail!("`RetiredCharacterLevels` not found");
            };
            *value = 1;
        }

        debug!(times_retired_after = ?props["TimesRetired"]);
        debug!(retired_character_levels_after = ?props["RetiredCharacterLevels"]);
    }

    save.write(&mut Cursor::new(&mut buf))?;
    fs::write(save_path, &buf)?;

    info!("replaced `{}` with modified save file", save_path.display());
    Ok(())
}
