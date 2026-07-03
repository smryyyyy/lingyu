fn main() {
    #[cfg(target_os = "windows")]
    {
        use std::collections::HashSet;
        use std::path::Path;
        use std::path::PathBuf;

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

        // Build an index of all DLLs in UCRT64 bin dir for quick resolution
        let mut dll_index: std::collections::HashMap<String, PathBuf> = std::collections::HashMap::new();
        if let Ok(entries) = std::fs::read_dir(bin_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".dll") {
                    dll_index.insert(name.to_lowercase(), entry.path());
                }
            }
        }

        // Find ntldd in the same bin dir
        let ntldd_path = bin_dir.join("ntldd.exe");
        let has_ntldd = ntldd_path.exists();

        // Resolve dependencies using ntldd (recursive)
        fn resolve_deps(
            binary: &Path,
            ntldd: &Path,
            dll_index: &std::collections::HashMap<String, PathBuf>,
            visited: &mut HashSet<String>,
            deps: &mut Vec<PathBuf>,
            bin_dir: &Path,
        ) {
            if !binary.exists() { return; }
            let key = binary.file_name().unwrap().to_string_lossy().to_lowercase();
            if !visited.insert(key) { return; }

            let output = std::process::Command::new(ntldd)
                .arg(binary)
                .output()
                .ok();
            let Some(output) = output else { return };
            if !output.status.success() { return; }
            let stdout = String::from_utf8_lossy(&output.stdout);

            for line in stdout.lines() {
                // ntldd output: "        libfoo.dll => /c/msys64/ucrt64/bin/libfoo.dll (0x...)"
                if let Some(idx) = line.find("=>") {
                    let path_part = line[idx + 2..].trim();
                    let dll_path = path_part.split_whitespace().next().unwrap_or("");
                    if dll_path.is_empty() || dll_path.starts_with('/') || !dll_path.ends_with(".dll") {
                        continue;
                    }
                    // Look up in bin dir
                    let name_lower = Path::new(dll_path).file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.to_lowercase())
                        .unwrap_or_default();
                    if let Some(full_path) = dll_index.get(&name_lower) {
                        if full_path.starts_with(bin_dir) {
                            let key2 = name_lower.clone();
                            if !visited.contains(&key2) {
                                deps.push(full_path.clone());
                                resolve_deps(full_path, ntldd, dll_index, visited, deps, bin_dir);
                            }
                        }
                    }
                }
            }
        }

        let exe_path = target_dir.join("lingyu.exe");
        let mut all_deps: Vec<PathBuf> = Vec::new();
        let mut visited = HashSet::new();

        if has_ntldd && exe_path.exists() {
            resolve_deps(&exe_path, &ntldd_path, &dll_index, &mut visited, &mut all_deps, bin_dir);
        }

        // Also resolve deps of gdk-pixbuf loader DLLs (loaded at runtime)
        let ucrt64_root = bin_dir.parent().unwrap();
        let loaders_src = ucrt64_root.join("lib\\gdk-pixbuf-2.0\\2.10.0\\loaders");
        if has_ntldd && loaders_src.exists() {
            if let Ok(entries) = std::fs::read_dir(&loaders_src) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("dll") {
                        resolve_deps(&path, &ntldd_path, &dll_index, &mut visited, &mut all_deps, bin_dir);
                    }
                }
            }
        }

        // Copy all resolved DLLs
        let mut copied = 0u32;
        for dll in &all_deps {
            let name = dll.file_name().unwrap();
            let dst = target_dir.join(name);
            if dst.exists() {
                continue;
            }
            if std::fs::copy(dll, &dst).is_ok() {
                copied += 1;
            }
        }

        if copied > 0 {
            println!("cargo:warning=LingYu: bundled {copied} DLLs to target/{profile}");
        }

        // ── Copy gdk-pixbuf format loaders (SVG, etc.) ──
        let ucrt64_root = bin_dir.parent().unwrap();
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
