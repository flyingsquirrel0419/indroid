//! `android:theme` of the <application> and each <activity> in AndroidManifest.xml.

use std::collections::HashMap;

use indroid_apk::axml::{Document, Element};
use indroid_apk::resources::ResValue;

/// Map from fully-qualified activity class ("" for the application) to theme id.
pub fn read(manifest: &[u8], package: &str) -> HashMap<String, u32> {
    let mut out = HashMap::new();
    let Ok(doc) = Document::parse(manifest) else {
        return out;
    };
    let theme = |e: &Element| match e.attribute("theme").map(|a| &a.value) {
        Some(ResValue::Reference(id)) => Some(*id),
        _ => None,
    };
    for app in doc.root.children.iter().filter(|e| e.name == "application") {
        if let Some(t) = theme(app) {
            out.insert(String::new(), t);
        }
        for act in app
            .children
            .iter()
            .filter(|e| e.name == "activity" || e.name == "activity-alias")
        {
            let Some(ResValue::String(name)) = act.attribute("name").map(|a| &a.value) else {
                continue;
            };
            let full = if name.starts_with('.') {
                format!("{package}{name}")
            } else if !name.contains('.') {
                format!("{package}.{name}")
            } else {
                name.clone()
            };
            if let Some(t) = theme(act) {
                out.insert(full, t);
            }
        }
    }
    out
}
