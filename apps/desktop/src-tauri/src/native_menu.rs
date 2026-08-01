use tauri::menu::{Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Runtime};

pub const EVENT_NAME: &str = "native-menu-action";
pub const ABOUT_VIVIDARIUM: &str = "app.about-vividarium";
pub const OPEN_PHOTO_LIBRARY: &str = "file.open-photo-library";
pub const MANAGE_PHOTO_LIBRARIES: &str = "file.manage-photo-libraries";
pub const OPEN_TAXONOMY_DATABASE: &str = "file.open-taxonomy-database";
pub const MANAGE_TAXONOMY_DATABASES: &str = "file.manage-taxonomy-databases";
pub const CLOSE_ALL_TABS: &str = "file.close-all-tabs";

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let menu = Menu::default(app)?;
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        replace_default_about(&menu, app)?;
        let file_menu = default_file_menu(&menu)?;
        let open_photo = MenuItem::with_id(
            app,
            OPEN_PHOTO_LIBRARY,
            "Open Photo Library\u{2026}",
            true,
            None::<&str>,
        )?;
        let manage_photos = MenuItem::with_id(
            app,
            MANAGE_PHOTO_LIBRARIES,
            "Manage Photo Libraries\u{2026}",
            true,
            None::<&str>,
        )?;
        let photo_separator = PredefinedMenuItem::separator(app)?;
        let open_taxonomy = MenuItem::with_id(
            app,
            OPEN_TAXONOMY_DATABASE,
            "Open Taxonomy Database\u{2026}",
            true,
            None::<&str>,
        )?;
        let manage_taxonomy = MenuItem::with_id(
            app,
            MANAGE_TAXONOMY_DATABASES,
            "Manage Taxonomy Databases\u{2026}",
            true,
            None::<&str>,
        )?;
        let taxonomy_separator = PredefinedMenuItem::separator(app)?;
        let close_tabs =
            MenuItem::with_id(app, CLOSE_ALL_TABS, "Close All Tabs", true, None::<&str>)?;
        let system_separator = PredefinedMenuItem::separator(app)?;
        file_menu.prepend_items(&[
            &open_photo,
            &manage_photos,
            &photo_separator,
            &open_taxonomy,
            &manage_taxonomy,
            &taxonomy_separator,
            &close_tabs,
            &system_separator,
        ])?;
    }
    Ok(menu)
}

pub fn handle<R: Runtime>(app: &AppHandle<R>, event: tauri::menu::MenuEvent) {
    let action = event.id().as_ref();
    if matches!(
        action,
        ABOUT_VIVIDARIUM
            | OPEN_PHOTO_LIBRARY
            | MANAGE_PHOTO_LIBRARIES
            | OPEN_TAXONOMY_DATABASE
            | MANAGE_TAXONOMY_DATABASES
            | CLOSE_ALL_TABS
    ) {
        let _ = app.emit_to("main", EVENT_NAME, action);
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn replace_default_about<R: Runtime>(menu: &Menu<R>, app: &AppHandle<R>) -> tauri::Result<()> {
    let about = MenuItem::with_id(
        app,
        ABOUT_VIVIDARIUM,
        "About Vividarium",
        true,
        None::<&str>,
    )?;
    for item in menu.items()? {
        if let MenuItemKind::Submenu(submenu) = item {
            for (index, child) in submenu.items()?.into_iter().enumerate() {
                if let MenuItemKind::Predefined(predefined) = child {
                    if predefined.text()?.replace('&', "").starts_with("About") {
                        submenu.remove_at(index)?;
                        submenu.insert(&about, index)?;
                        return Ok(());
                    }
                }
            }
        }
    }
    Err(std::io::Error::other("default About menu item is missing").into())
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn default_file_menu<R: Runtime>(menu: &Menu<R>) -> tauri::Result<Submenu<R>> {
    for item in menu.items()? {
        if let MenuItemKind::Submenu(submenu) = item {
            if submenu.text()? == "File" {
                return Ok(submenu);
            }
        }
    }
    Err(std::io::Error::other("default File menu is missing").into())
}
