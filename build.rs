fn main() {
    #[cfg(target_os = "windows")]
    {
        // Auto-bundle GTK4 DLLs from MSYS2 UCRT64 into the build output.
        // After `cargo build`, the .exe and all required DLLs are in
        // target/debug/ or target/release/, ready to run.
        let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
        let target_dir = std::path::Path::new("target").join(&profile);
        std::fs::create_dir_all(&target_dir).ok();

        // Common MSYS2 UCRT64 install paths
        let candidates = [
            r"C:\msys64\ucrt64\bin",
            r"C:\tools\msys64\ucrt64\bin",
            r"C:\msys2\ucrt64\bin",
        ];
        let ucrt64_bin = candidates.iter().find(|p| std::path::Path::new(p).join("libgtk-4-1.dll").exists());

        if let Some(bin_dir) = ucrt64_bin {
            let bin_dir = std::path::Path::new(bin_dir);

            // Core GTK4 DLLs (the minimal set; gtk4-rs will pull in more via dependency)
            let dlls = [
                "libgtk-4-1.dll",
                "libgdk-4-1.dll",
                "libgdk_pixbuf-2.0-0.dll",
                "libpangocairo-1.0-0.dll",
                "libpango-1.0-0.dll",
                "libpangowin32-1.0-0.dll",
                "libharfbuzz-0.dll",
                "libcairo-2.dll",
                "libcairo-gobject-2.dll",
                "libgio-2.0-0.dll",
                "libglib-2.0-0.dll",
                "libgobject-2.0-0.dll",
                "libgmodule-2.0-0.dll",
                "libintl-8.dll",
                "libpcre2-8-0.dll",
                "libffi-8.dll",
                "libepoxy-0.dll",
                "libfribidi-0.dll",
                "libpixman-1-0.dll",
                "libpng16-16.dll",
                "libgraphene-1.0-0.dll",
                "zlib1.dll",
                "libstdc++-6.dll",      // C++ runtime (GTK/GDK have C++ deps)
                "libwinpthread-1.dll",   // MinGW pthread
                "libgcc_s_seh-1.dll",    // MinGW GCC runtime
            ];

            let mut copied = 0;
            for dll in &dlls {
                let src = bin_dir.join(dll);
                let dst = target_dir.join(dll);
                if src.exists() && !dst.exists() {
                    if std::fs::copy(&src, &dst).is_ok() {
                        copied += 1;
                    }
                }
            }
            if copied > 0 {
                println!("cargo:warning=LingYu: bundled {} GTK4 DLLs to target/{profile}", copied);
            }
        } else {
            println!("cargo:warning=LingYu: MSYS2 UCRT64 not found at C:\\msys64\\ucrt64\\bin. Run `just deps` in an MSYS2 UCRT64 terminal first.");
        }
    }
}
