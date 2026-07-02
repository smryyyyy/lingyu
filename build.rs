fn main() {
    #[cfg(target_os = "windows")]
    {
        use std::path::Path;

        let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
        let target_dir = Path::new("target").join(&profile);
        std::fs::create_dir_all(&target_dir).ok();

        // Common MSYS2 UCRT64 install paths
        let candidates = [
            r"C:\msys64\ucrt64\bin",
            r"C:\tools\msys64\ucrt64\bin",
            r"C:\msys2\ucrt64\bin",
        ];
        let ucrt64_bin = candidates
            .iter()
            .find(|p| Path::new(p).join("libgtk-4-1.dll").exists());

        let Some(bin_dir) = ucrt64_bin else {
            println!("cargo:warning=LingYu: MSYS2 UCRT64 not found. Build will work but the .exe won't run without GTK4 DLLs. Run `just deps` in an MSYS2 UCRT64 terminal first.");
            return;
        };
        let bin_dir = Path::new(bin_dir);

        // Scan UCRT64 bin dir for GTK4-related DLLs (auto-detects version changes)
        let mut copied = 0u32;
        if let Ok(entries) = std::fs::read_dir(bin_dir) {
            for entry in entries.flatten() {
                let Some(name) = entry.file_name().to_str().map(|s| s.to_string()) else { continue };
                if !name.ends_with(".dll") {
                    continue;
                }
                let lower = name.to_lowercase();

                // Match known GTK4/mingw dependency prefixes
                let is_gtk_dep = lower.starts_with("libgtk-4")
                    || lower.starts_with("libgdk-4")
                    || lower.starts_with("libgdk_pixbuf")
                    || lower.starts_with("libglib-2.0")
                    || lower.starts_with("libgobject-2.0")
                    || lower.starts_with("libgio-2.0")
                    || lower.starts_with("libgmodule-2.0")
                    || lower.starts_with("libpango")
                    || lower.starts_with("libpangocairo")
                    || lower.starts_with("libpangowin32")
                    || lower.starts_with("libcairo")
                    || lower.starts_with("libharfbuzz")
                    || lower.starts_with("libfribidi")
                    || lower.starts_with("libpixman")
                    || lower.starts_with("libpng")
                    || lower.starts_with("libepoxy")
                    || lower.starts_with("libgraphene")
                    || lower.starts_with("libpcre2")
                    || lower.starts_with("libffi")
                    || lower.starts_with("libintl")
                    || lower.starts_with("libiconv")
                    || lower.starts_with("libstdc++")
                    || lower.starts_with("libwinpthread")
                    || lower.starts_with("libgcc_s")
                    || lower == "zlib1.dll";

                if !is_gtk_dep {
                    continue;
                }

                let dst = target_dir.join(&name);
                if dst.exists() {
                    continue; // already copied on a previous build
                }
                if std::fs::copy(&entry.path(), &dst).is_ok() {
                    copied += 1;
                }
            }
        }

        if copied > 0 {
            println!("cargo:warning=LingYu: bundled {copied} GTK4 DLLs to target/{profile}");
        }

        // ── Copy gdk-pixbuf format loaders (SVG, etc.) ──
        let ucrt64_root = bin_dir.parent().unwrap(); // e.g. C:\msys64\ucrt64
        let loaders_src = ucrt64_root
            .join("lib\\gdk-pixbuf-2.0\\2.10.0\\loaders");
        let loaders_dst = target_dir.join("lib\\gdk-pixbuf-2.0\\2.10.0\\loaders");
        let query_loaders = ucrt64_root.join("bin\\gdk-pixbuf-query-loaders.exe");

        if loaders_src.exists() && query_loaders.exists() {
            std::fs::create_dir_all(&loaders_dst).ok();
            let mut loader_copied = 0u32;
            if let Ok(entries) = std::fs::read_dir(&loaders_src) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("dll") {
                        let name = entry.file_name();
                        let dst = loaders_dst.join(&name);
                        if !dst.exists() && std::fs::copy(&path, &dst).is_ok() {
                            loader_copied += 1;
                        }
                    }
                }
            }
            if loader_copied > 0 {
                println!("cargo:warning=LingYu: bundled {loader_copied} gdk-pixbuf loaders to target/{profile}");

                // Generate loaders.cache using MSYS2's gdk-pixbuf-query-loaders
                let cache_path = target_dir.join("lib\\gdk-pixbuf-2.0\\2.10.0\\loaders.cache");
                if !cache_path.exists() {
                    if let Ok(output) = std::process::Command::new(&query_loaders).output() {
                        if output.status.success() {
                            let cache_content = String::from_utf8_lossy(&output.stdout);
                            if let Err(e) = std::fs::write(&cache_path, cache_content.as_bytes()) {
                                println!("cargo:warning=LingYu: failed to write loaders.cache: {e}");
                            } else {
                                println!("cargo:warning=LingYu: generated loaders.cache");
                            }
                        } else {
                            println!("cargo:warning=LingYu: gdk-pixbuf-query-loaders failed");
                        }
                    } else {
                        println!("cargo:warning=LingYu: could not run gdk-pixbuf-query-loaders");
                    }
                }
            }
        }
    }
}
