use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSDictionary, NSObject, NSProcessInfo, NSString, NSURL};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub(crate) fn registered_app_paths(bundle_id: &str) -> Vec<PathBuf> {
    objc2::rc::autoreleasepool(|_| {
        let workspace = NSWorkspace::sharedWorkspace();
        let identifier = NSString::from_str(bundle_id);
        if NSProcessInfo::processInfo()
            .operatingSystemVersion()
            .majorVersion
            < 12
        {
            return workspace
                .URLForApplicationWithBundleIdentifier(&identifier)
                .iter()
                .filter_map(|url| matching_app_path(url, bundle_id))
                .collect();
        }
        workspace
            .URLsForApplicationsWithBundleIdentifier(&identifier)
            .iter()
            .filter_map(|url| matching_app_path(&url, bundle_id))
            .collect()
    })
}

fn matching_app_path(url: &NSURL, bundle_id: &str) -> Option<PathBuf> {
    if !url.isFileURL() {
        return None;
    }
    let path = PathBuf::from(url.path()?.to_string());
    if !path.is_dir() || !path.extension()?.eq_ignore_ascii_case("app") {
        return None;
    }
    let info = path.join("Contents/Info.plist");
    let info_url = NSURL::fileURLWithPath(&NSString::from_str(info.to_str()?));
    // NSBundle caches identifiers by path. Read the plist directly, whose values are NSObjects.
    let metadata =
        unsafe { NSDictionary::<NSString, NSObject>::dictionaryWithContentsOfURL_error(&info_url) }
            .ok()?;
    let value = |key: &str| {
        let string = metadata
            .objectForKey(&NSString::from_str(key))?
            .downcast::<NSString>()
            .ok()?;
        Some(string.to_string())
    };
    if !value("CFBundleIdentifier")?.eq_ignore_ascii_case(bundle_id) {
        return None;
    }
    let executable = value("CFBundleExecutable")?;
    let executable = Path::new(&executable);
    if executable.file_name()? != executable.as_os_str() {
        return None;
    }
    let launcher = path
        .join("Contents/MacOS")
        .join(executable)
        .metadata()
        .ok()?;
    (launcher.is_file() && launcher.permissions().mode() & 0o111 != 0).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::installer::TempInstallDir;

    #[test]
    fn registered_url_must_still_point_to_a_matching_app() {
        let root = TempInstallDir::create_in(&std::env::temp_dir()).unwrap();
        let app = root.path().join("Custom Path/Agent 测试.app");
        let info = app.join("Contents/Info.plist");
        let executable = app.join("Contents/MacOS/agent");
        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        std::fs::write(&executable, "").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        let write_id = |id: &str| {
            std::fs::write(
                &info,
                format!(
                    r#"<plist version="1.0"><dict><key>CFBundleIdentifier</key><string>{id}</string><key>CFBundleExecutable</key><string>agent</string></dict></plist>"#
                ),
            )
            .unwrap();
        };
        let url = NSURL::fileURLWithPath(&NSString::from_str(app.to_str().unwrap()));
        let id = "com.agentbro.test-original";

        write_id(id);
        assert_eq!(matching_app_path(&url, id), Some(app));
        write_id("com.agentbro.test-replacement");
        assert_eq!(matching_app_path(&url, id), None);
        write_id(id);
        std::fs::remove_file(executable).unwrap();
        assert_eq!(matching_app_path(&url, id), None);
    }
}
