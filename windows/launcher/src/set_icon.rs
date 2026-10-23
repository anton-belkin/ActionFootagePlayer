// SPDX-License-Identifier: GPL-3.0-or-later
//
// `set-icon <exe> <ico> [<group>]`: puts the icon in <ico> into an executable, in place of its own; a build tool
// (tools/make_windows_app.sh runs it on mpv.exe and ActionFootagePlayer.exe). <group> is the name of the icon group
// to replace, default IDI_ICON1: mpv's, which it loads its window icon by (so the window and the taskbar show this
// one). rcedit can't do it: it takes icon groups by number. The group keeps its name and language; the images
// replace the old ones.

#[cfg(windows)]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 || args.len() > 3 {
        eprintln!("usage: set-icon <exe> <ico> [<group>]");
        std::process::exit(2);
    }
    let group = args.get(2).map(String::as_str).unwrap_or("IDI_ICON1");
    if let Err(e) = win::set_icon(&args[0], &args[1], group) {
        eprintln!("set-icon: {e}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("set-icon is for Windows");
    std::process::exit(1);
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    type Handle = *mut c_void;
    const RT_ICON: usize = 3;
    const RT_GROUP_ICON: usize = 14;
    const LOAD_LIBRARY_AS_DATAFILE: u32 = 0x2;
    // English (US), when the executable has no icon of its own yet
    const DEFAULT_LANG: u16 = 1033;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryExW(name: *const u16, file: Handle, flags: u32) -> Handle;
        fn FreeLibrary(module: Handle) -> i32;
        fn EnumResourceLanguagesW(module: Handle, kind: *const u16, name: *const u16,
                                  callback: extern "system" fn(Handle, *const u16, *const u16, u16, isize) -> i32,
                                  param: isize) -> i32;
        fn EnumResourceNamesW(module: Handle, kind: *const u16,
                              callback: extern "system" fn(Handle, *const u16, *const u16, isize) -> i32,
                              param: isize) -> i32;
        fn BeginUpdateResourceW(file: *const u16, delete_existing: i32) -> Handle;
        fn UpdateResourceW(update: Handle, kind: *const u16, name: *const u16, lang: u16, data: *const c_void,
                           size: u32) -> i32;
        fn EndUpdateResourceW(update: Handle, discard: i32) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    // MAKEINTRESOURCE
    fn id(n: usize) -> *const u16 {
        n as *const u16
    }

    extern "system" fn first_lang(_: Handle, _: *const u16, _: *const u16, lang: u16, param: isize) -> i32 {
        unsafe { *(param as *mut Option<u16>) = Some(lang) };
        0
    }

    extern "system" fn icon_ids(_: Handle, _: *const u16, name: *const u16, param: isize) -> i32 {
        // IS_INTRESOURCE: the images are numbered
        if (name as usize) >> 16 == 0 {
            unsafe { (*(param as *mut Vec<usize>)).push(name as usize) };
        }
        1
    }

    struct Image<'a> {
        entry: &'a [u8],
        data: &'a [u8],
    }

    // An .ico: ICONDIR (6 bytes), an ICONDIRENTRY per image (16 bytes, the last 4 its offset in the file)
    fn read_ico(ico: &[u8]) -> Result<Vec<Image<'_>>, String> {
        let u16_at = |i: usize| ico.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
        let u32_at = |i: usize| ico.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
        if u16_at(0) != Some(0) || u16_at(2) != Some(1) {
            return Err("not an .ico".into());
        }
        let count = u16_at(4).unwrap_or(0) as usize;
        (0..count).map(|i| {
            let at = 6 + 16 * i;
            let entry = ico.get(at..at + 16).ok_or("truncated .ico")?;
            let (size, offset) = (u32_at(at + 8).unwrap(), u32_at(at + 12).unwrap());
            let data = ico.get(offset..offset + size).ok_or("truncated .ico")?;
            Ok(Image { entry, data })
        }).collect()
    }

    pub fn set_icon(exe: &str, ico: &str, group: &str) -> Result<(), String> {
        let bytes = std::fs::read(ico).map_err(|e| format!("{ico}: {e}"))?;
        let images = read_ico(&bytes).map_err(|e| format!("{ico}: {e}"))?;
        if images.is_empty() {
            return Err(format!("{ico}: no images"));
        }
        let exe_w = wide(exe);
        let group_w = wide(group);

        // The group's language, and the images the executable has now
        let mut lang: Option<u16> = None;
        let mut old_ids: Vec<usize> = Vec::new();
        unsafe {
            let module = LoadLibraryExW(exe_w.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_AS_DATAFILE);
            if module.is_null() {
                return Err(format!("{exe}: {}", std::io::Error::last_os_error()));
            }
            EnumResourceLanguagesW(module, id(RT_GROUP_ICON), group_w.as_ptr(), first_lang,
                                   &mut lang as *mut _ as isize);
            EnumResourceNamesW(module, id(RT_ICON), icon_ids, &mut old_ids as *mut _ as isize);
            FreeLibrary(module);
        }
        let lang = lang.unwrap_or(DEFAULT_LANG);

        // GRPICONDIR: ICONDIR, then per image its ICONDIRENTRY with the image's resource id in place of the offset
        let mut grp: Vec<u8> = Vec::new();
        grp.extend_from_slice(&[0, 0, 1, 0]);
        grp.extend_from_slice(&(images.len() as u16).to_le_bytes());
        for (i, image) in images.iter().enumerate() {
            grp.extend_from_slice(&image.entry[..12]);
            grp.extend_from_slice(&(i as u16 + 1).to_le_bytes());
        }

        unsafe {
            let update = BeginUpdateResourceW(exe_w.as_ptr(), 0);
            if update.is_null() {
                return Err(format!("{exe}: {}", std::io::Error::last_os_error()));
            }
            let mut ok = true;
            for (i, image) in images.iter().enumerate() {
                ok &= UpdateResourceW(update, id(RT_ICON), id(i + 1), lang, image.data.as_ptr().cast(),
                                      image.data.len() as u32) != 0;
            }
            // The old images the new ones don't take the place of
            for &old in old_ids.iter().filter(|&&n| n > images.len()) {
                UpdateResourceW(update, id(RT_ICON), id(old), lang, std::ptr::null(), 0);
            }
            ok &= UpdateResourceW(update, id(RT_GROUP_ICON), group_w.as_ptr(), lang, grp.as_ptr().cast(),
                                  grp.len() as u32) != 0;
            if !ok {
                let e = std::io::Error::last_os_error();
                EndUpdateResourceW(update, 1);
                return Err(format!("{exe}: {e}"));
            }
            if EndUpdateResourceW(update, 0) == 0 {
                return Err(format!("{exe}: {}", std::io::Error::last_os_error()));
            }
        }
        Ok(())
    }
}
