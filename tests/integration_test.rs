//! All integration tests are skipped by default (using the `ignore` attribute)
//! since performing builds is slow. To run them use: `cargo test -- --ignored`.

// Required due to: https://github.com/rust-lang/rust/issues/95513
#![allow(unused_crate_dependencies)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::str::FromStr;

use insta::{assert_snapshot, with_settings};
use libcnb_test::{BuildConfig, BuildpackReference, PackResult, TestContext, TestRunner, assert_contains, assert_contains_match, assert_not_contains};
use toml_edit::{Array, DocumentMut, InlineTable, value};

#[test]
#[ignore = "integration test"]
fn test_successful_detection() {
    integration_test_with_config(
        "fixtures/project_file_with_empty_config",
        |config| {
            config.expected_pack_result(PackResult::Success);
        },
        |_| {},
    );
}

#[test]
#[ignore = "integration test"]
fn test_failed_detection_when_no_project_file_exists() {
    integration_test_with_config(
        "fixtures/no_project_file",
        |config| {
            config.expected_pack_result(PackResult::Failure);
        },
        |ctx| {
            assert_contains!(ctx.pack_stdout, "No project.toml or Aptfile found.");
        },
    );
}

#[test]
#[ignore = "integration test"]
fn test_failed_detection_when_project_file_has_no_config() {
    integration_test_with_config(
        "fixtures/project_file_with_no_config",
        |config| {
            config.expected_pack_result(PackResult::Failure);
        },
        |ctx| {
            assert_contains!(ctx.pack_stdout, "project.toml found, but no [com.heroku.buildpacks.deb-packages] configuration present.");
        },
    );
}

#[test]
#[ignore = "integration test"]
fn test_passes_detection_when_project_file_with_empty_config_exists_and_prints_help_during_build() {
    integration_test("fixtures/project_file_with_empty_config", |ctx| {
        assert_contains!(ctx.pack_stdout, "No configured packages to install found in project.toml file.");
    });
}

#[test]
#[ignore = "integration test"]
fn test_aptfile_passes_detection_and_prints_help_during_build() {
    integration_test_with_config(
        "fixtures/no_project_file",
        |config| {
            config.expected_pack_result(PackResult::Success);
            config.app_dir_preprocessor(|dir| {
                std::fs::write(dir.join("Aptfile"), "").unwrap();
            });
        },
        |ctx| {
            assert_contains!(ctx.pack_stdout, "The use of an `Aptfile` is deprecated!");
        },
    );
}

#[test]
#[ignore = "integration test"]
fn test_general_usage_output() {
    integration_test("fixtures/general_usage", |ctx| {
        create_build_snapshot(&ctx.pack_stdout).assert();
    });
}

#[test]
#[ignore = "integration test"]
#[allow(clippy::too_many_lines)]
fn test_general_usage_output_when_buildpack_log_level_is_debug() {
    integration_test_with_config(
        "fixtures/general_usage",
        |config| {
            config.env("BP_LOG_LEVEL", "DEBUG");
        },
        |ctx| {
            let multiarch_name = match get_integration_test_arch().as_str() {
                "amd64" => "x86_64-linux-gnu",
                "arm64" => "aarch64-linux-gnu",
                _ => panic_unsupported_test_configuration(),
            };
            assert_contains!(ctx.pack_stdout, "Layer file listing");
            assert_contains!(ctx.pack_stdout, "/layers/heroku_deb-packages/packages/usr/bin/xmlsec1");
            assert_contains!(ctx.pack_stdout, &format!("/layers/heroku_deb-packages/packages/usr/lib/{multiarch_name}/libgwenhywfar.so"));
        },
    );
}

#[test]
#[ignore = "integration test"]
#[allow(clippy::too_many_lines)]
fn test_general_usage_output_on_rebuild() {
    integration_test("fixtures/general_usage", |ctx| {
        let build_snapshot = create_build_snapshot(&ctx.pack_stdout);
        let config = ctx.config.clone();
        ctx.rebuild(config, |ctx| {
            build_snapshot.rebuild_output(&ctx.pack_stdout).assert();
        });
    });
}

#[test]
#[ignore = "integration test"]
#[allow(clippy::too_many_lines)]
fn test_general_usage_env() {
    integration_test("fixtures/general_usage", |ctx| {
        let layer_path = "/layers/heroku_deb-packages/packages";

        let path = get_env_var(&ctx, "PATH");
        let ld_library_path = get_env_var(&ctx, "LD_LIBRARY_PATH");
        let library_path = get_env_var(&ctx, "LIBRARY_PATH");
        let include_path = get_env_var(&ctx, "INCLUDE_PATH");
        let cpath = get_env_var(&ctx, "CPATH");
        let cpp_path = get_env_var(&ctx, "CPPPATH");
        let pkg_config_path = get_env_var(&ctx, "PKG_CONFIG_PATH");

        assert_eq!(ld_library_path, library_path);
        assert_eq!(include_path, cpath);
        assert_eq!(include_path, cpp_path);

        match (get_integration_test_builder().as_str(), get_integration_test_arch().as_str()) {
            (_, "amd64") => {
                assert_contains!(path, &format!("{layer_path}/bin"));
                assert_contains!(path, &format!("{layer_path}/usr/bin"));
                assert_contains!(path, &format!("{layer_path}/usr/sbin"));
                assert_contains!(ld_library_path, &format!("{layer_path}/usr/lib/x86_64-linux-gnu"));
                assert_contains!(ld_library_path, &format!("{layer_path}/usr/lib"));
                assert_contains!(ld_library_path, &format!("{layer_path}/lib/x86_64-linux-gnu"));
                assert_contains!(ld_library_path, &format!("{layer_path}/lib"));
                assert_contains!(include_path, &format!("{layer_path}/usr/include/x86_64-linux-gnu"));
                assert_contains!(include_path, &format!("{layer_path}/usr/include"));
                assert_contains!(pkg_config_path, &format!("{layer_path}/usr/lib/x86_64-linux-gnu/pkgconfig"));
                assert_contains!(pkg_config_path, &format!("{layer_path}/usr/lib/pkgconfig"));
            }
            (_, "arm64") => {
                assert_contains!(path, &format!("{layer_path}/bin"));
                assert_contains!(path, &format!("{layer_path}/usr/bin"));
                assert_contains!(path, &format!("{layer_path}/usr/sbin"));
                assert_contains!(ld_library_path, &format!("{layer_path}/usr/lib/aarch64-linux-gnu"));
                assert_contains!(ld_library_path, &format!("{layer_path}/usr/lib"));
                assert_contains!(ld_library_path, &format!("{layer_path}/lib/aarch64-linux-gnu"));
                assert_contains!(ld_library_path, &format!("{layer_path}/lib"));
                assert_contains!(include_path, &format!("{layer_path}/usr/include/aarch64-linux-gnu"));
                assert_contains!(include_path, &format!("{layer_path}/usr/include"));
                assert_contains!(pkg_config_path, &format!("{layer_path}/usr/lib/aarch64-linux-gnu/pkgconfig"));
                assert_contains!(pkg_config_path, &format!("{layer_path}/usr/lib/pkgconfig"));
            }
            _ => panic_unsupported_test_configuration(),
        }
    });
}

#[test]
#[ignore = "integration test"]
fn test_package_config_rewrite() {
    integration_test_with_config(
        "fixtures/project_file_with_empty_config",
        |config| {
            config.app_dir_preprocessor(|app_dir| {
                set_install_config(&app_dir, [requested_package_config("libopusfile-dev", true)]);
            });
        },
        |ctx| match (get_integration_test_builder().as_str(), get_integration_test_arch().as_str()) {
            ("heroku/builder:22", "amd64") => {
                assert_contains!(read_package_config(&ctx, "usr/lib/pkgconfig/opusfile.pc"), "prefix=/layers/heroku_deb-packages/packages/usr");
                assert_contains!(read_package_config(&ctx, "usr/lib/pkgconfig/opusurl.pc"), "prefix=/layers/heroku_deb-packages/packages/usr");
            }
            ("heroku/builder:24" | "heroku/builder:26", "amd64") => {
                assert_contains!(read_package_config(&ctx, "usr/lib/x86_64-linux-gnu/pkgconfig/opusfile.pc"), "prefix=/layers/heroku_deb-packages/packages/usr");
                assert_contains!(read_package_config(&ctx, "usr/lib/x86_64-linux-gnu/pkgconfig/opusurl.pc"), "prefix=/layers/heroku_deb-packages/packages/usr");
            }
            ("heroku/builder:24" | "heroku/builder:26", "arm64") => {
                assert_contains!(read_package_config(&ctx, "usr/lib/aarch64-linux-gnu/pkgconfig/opusfile.pc"), "prefix=/layers/heroku_deb-packages/packages/usr");
                assert_contains!(read_package_config(&ctx, "usr/lib/aarch64-linux-gnu/pkgconfig/opusurl.pc"), "prefix=/layers/heroku_deb-packages/packages/usr");
            }
            _ => panic_unsupported_test_configuration(),
        },
    );
}

#[test]
#[ignore = "integration test"]
#[allow(clippy::match_same_arms)]
fn test_cache_invalidated_when_configuration_changes() {
    integration_test_with_config(
        "fixtures/project_file_with_empty_config",
        |config| {
            config.app_dir_preprocessor(|app_dir| {
                let package = match get_integration_test_builder().as_str() {
                    "heroku/builder:26" => "libxmlsec1-1",
                    _ => "libxmlsec1",
                };
                set_install_config(&app_dir, [requested_package_config(package, true)]);
            });
        },
        |ctx| {
            match (get_integration_test_builder().as_str(), get_integration_test_arch().as_str()) {
                ("heroku/builder:22", "amd64") => {
                    assert_contains!(ctx.pack_stdout, "Requesting packages");
                    assert_contains!(ctx.pack_stdout, "Adding `libxmlsec1@1.2.33-1build2`");
                    assert_contains!(ctx.pack_stdout, "`libxmlsec1@1.2.33-1build2` from http://archive.ubuntu.com/ubuntu/pool/main/x/xmlsec1/libxmlsec1_1.2.33-1build2_amd64.deb");

                    assert_not_contains!(ctx.pack_stdout, "Adding `libgwenhywfar-data@5.9.0-1`");
                    assert_not_contains!(ctx.pack_stdout, "`libgwenhywfar-data@5.9.0-1` from http://archive.ubuntu.com/ubuntu/pool/universe/libg/libgwenhywfar/libgwenhywfar-data_5.9.0-1_all.deb");
                }
                ("heroku/builder:24", "amd64") => {
                    assert_contains!(ctx.pack_stdout, "Requesting packages");
                    assert_contains!(ctx.pack_stdout, "Adding `libxmlsec1t64@1.2.39-5build2`");
                    assert_contains!(ctx.pack_stdout, "`libxmlsec1t64@1.2.39-5build2` from http://archive.ubuntu.com/ubuntu/pool/main/x/xmlsec1/libxmlsec1t64_1.2.39-5build2_amd64.deb");

                    assert_not_contains!(ctx.pack_stdout, "Adding `libgwenhywfar-data@5.10.2-2.1build4`");
                    assert_not_contains!(ctx.pack_stdout, "`libgwenhywfar-data@5.10.2-2.1build4` from http://archive.ubuntu.com/ubuntu/pool/universe/libg/libgwenhywfar/libgwenhywfar-data@5.10.2-2.1build4_all.deb");
                }
                ("heroku/builder:24", "arm64") => {
                    assert_contains!(ctx.pack_stdout, "Requesting packages");
                    assert_contains!(ctx.pack_stdout, "Adding `libxmlsec1t64@1.2.39-5build2`");
                    assert_contains!(ctx.pack_stdout, "`libxmlsec1t64@1.2.39-5build2` from http://ports.ubuntu.com/ubuntu-ports/pool/main/x/xmlsec1/libxmlsec1t64_1.2.39-5build2_arm64.deb");

                    assert_not_contains!(ctx.pack_stdout, "Adding `libgwenhywfar-data@5.10.2-2.1build4`");
                    assert_not_contains!(ctx.pack_stdout, "`libgwenhywfar-data@5.10.2-2.1build4` from http://ports.ubuntu.com/ubuntu-ports/pool/universe/libg/libgwenhywfar/libgwenhywfar-data_5.10.2-2.1build4_all.deb");
                }
                ("heroku/builder:26", "amd64") => {
                    assert_contains!(ctx.pack_stdout, "Requesting packages");
                    assert_contains!(ctx.pack_stdout, "Adding `libxmlsec1-1@1.3.9-1`");
                    assert_contains!(ctx.pack_stdout, "`libxmlsec1-1@1.3.9-1` from http://archive.ubuntu.com/ubuntu/pool/main/x/xmlsec1/libxmlsec1-1_1.3.9-1_amd64.deb");

                    assert_not_contains!(ctx.pack_stdout, "Adding `libgwenhywfar-data@5.14.1-2`");
                    assert_not_contains!(ctx.pack_stdout, "`libgwenhywfar-data@5.14.1-2` from http://archive.ubuntu.com/ubuntu/pool/universe/libg/libgwenhywfar/libgwenhywfar-data_5.14.1-2_all.deb");
                }
                ("heroku/builder:26", "arm64") => {
                    assert_contains!(ctx.pack_stdout, "Requesting packages");
                    assert_contains!(ctx.pack_stdout, "Adding `libxmlsec1-1@1.3.9-1`");
                    assert_contains!(ctx.pack_stdout, "`libxmlsec1-1@1.3.9-1` from http://ports.ubuntu.com/ubuntu-ports/pool/main/x/xmlsec1/libxmlsec1-1_1.3.9-1_arm64.deb");

                    assert_not_contains!(ctx.pack_stdout, "Adding `libgwenhywfar-data@5.14.1-2`");
                    assert_not_contains!(ctx.pack_stdout, "`libgwenhywfar-data@5.14.1-2` from http://ports.ubuntu.com/ubuntu-ports/pool/universe/libg/libgwenhywfar/libgwenhywfar-data_5.14.1-2_all.deb");
                }
                _ => panic_unsupported_test_configuration(),
            }

            let mut config = ctx.config.clone();
            ctx.rebuild(
                config.app_dir_preprocessor(|app_dir| {
                    set_install_config(&app_dir, [requested_package_config("libgwenhywfar-data", true)]);
                }),
                |ctx| match (get_integration_test_builder().as_str(), get_integration_test_arch().as_str()) {
                    ("heroku/builder:22", "amd64") => {
                        assert_contains!(ctx.pack_stdout, "Requesting packages (packages changed)");
                        assert_contains!(ctx.pack_stdout, "Adding `libgwenhywfar-data@5.9.0-1`");
                        assert_contains!(ctx.pack_stdout, "`libgwenhywfar-data@5.9.0-1` from http://archive.ubuntu.com/ubuntu/pool/universe/libg/libgwenhywfar/libgwenhywfar-data_5.9.0-1_all.deb");

                        assert_not_contains!(ctx.pack_stdout, "Adding `libxmlsec1@1.2.33-1build2`");
                        assert_not_contains!(ctx.pack_stdout, "`libxmlsec1@1.2.33-1build2` from http://archive.ubuntu.com/ubuntu/pool/main/x/xmlsec1/libxmlsec1_1.2.33-1build2_amd64.deb");
                    }
                    ("heroku/builder:24", "amd64") => {
                        assert_contains!(ctx.pack_stdout, "Requesting packages (packages changed)");
                        assert_contains!(ctx.pack_stdout, "Adding `libgwenhywfar-data@5.10.2-2.1build4`");
                        assert_contains!(ctx.pack_stdout, "`libgwenhywfar-data@5.10.2-2.1build4` from http://archive.ubuntu.com/ubuntu/pool/universe/libg/libgwenhywfar/libgwenhywfar-data_5.10.2-2.1build4_all.deb");

                        assert_not_contains!(ctx.pack_stdout, "Adding `libxmlsec1t64@1.2.39-5build2`");
                        assert_not_contains!(ctx.pack_stdout, "`libxmlsec1t64@1.2.39-5build2` from http://archive.ubuntu.com/ubuntu/pool/main/x/xmlsec1/libxmlsec1t64@1.2.39-5build2_amd64.deb");
                    }
                    ("heroku/builder:24", "arm64") => {
                        assert_contains!(ctx.pack_stdout, "Requesting packages (packages changed)");
                        assert_contains!(ctx.pack_stdout, "Adding `libgwenhywfar-data@5.10.2-2.1build4`");
                        assert_contains!(ctx.pack_stdout, "`libgwenhywfar-data@5.10.2-2.1build4` from http://ports.ubuntu.com/ubuntu-ports/pool/universe/libg/libgwenhywfar/libgwenhywfar-data_5.10.2-2.1build4_all.deb");

                        assert_not_contains!(ctx.pack_stdout, "Adding `libxmlsec1t64@1.2.39-5build2`");
                        assert_not_contains!(ctx.pack_stdout, "`libxmlsec1t64@1.2.39-5build2` from http://ports.ubuntu.com/ubuntu-ports/pool/main/x/xmlsec1/libxmlsec1t64_1.2.39-5build2_arm64.deb");
                    }
                    ("heroku/builder:26", "amd64") => {
                        assert_contains!(ctx.pack_stdout, "Requesting packages (packages changed)");
                        assert_contains!(ctx.pack_stdout, "Adding `libgwenhywfar-data@5.14.1-2`");
                        assert_contains!(ctx.pack_stdout, "`libgwenhywfar-data@5.14.1-2` from http://archive.ubuntu.com/ubuntu/pool/universe/libg/libgwenhywfar/libgwenhywfar-data_5.14.1-2_all.deb");

                        assert_not_contains!(ctx.pack_stdout, "Adding `libxmlsec1-1@1.3.9-1`");
                        assert_not_contains!(ctx.pack_stdout, "`libxmlsec1-1@1.3.9-1` from http://archive.ubuntu.com/ubuntu/pool/main/x/xmlsec1/libxmlsec1-1_1.3.9-1_amd64.deb");
                    }
                    ("heroku/builder:26", "arm64") => {
                        assert_contains!(ctx.pack_stdout, "Requesting packages (packages changed)");
                        assert_contains!(ctx.pack_stdout, "Adding `libgwenhywfar-data@5.14.1-2`");
                        assert_contains!(ctx.pack_stdout, "`libgwenhywfar-data@5.14.1-2` from http://ports.ubuntu.com/ubuntu-ports/pool/universe/libg/libgwenhywfar/libgwenhywfar-data_5.14.1-2_all.deb");

                        assert_not_contains!(ctx.pack_stdout, "Adding `libxmlsec1-1@1.3.9-1`");
                        assert_not_contains!(ctx.pack_stdout, "`libxmlsec1-1@1.3.9-1` from http://ports.ubuntu.com/ubuntu-ports/pool/main/x/xmlsec1/libxmlsec1-1_1.3.9-1_arm64.deb");
                    }
                    _ => panic_unsupported_test_configuration(),
                },
            );
        },
    );
}

#[test]
#[ignore = "integration test"]
fn ffmpeg_usage() {
    integration_test_with_config(
        "fixtures/project_file_with_empty_config",
        |config| {
            config.app_dir_preprocessor(|app_dir| {
                set_install_config(&app_dir, [requested_package_config("ffmpeg", false)]);
            });
        },
        |ctx| {
            assert_contains!(ctx.run_shell_command("ffmpeg -version").stdout, "ffmpeg version");
        },
    );
}

#[test]
#[ignore = "integration test"]
fn geo_buildpack_use_case() {
    if get_integration_test_builder().as_str() != "heroku/builder:22" {
        return;
    }
    // this test is supposed to validate that this buildpack can be an effective replacement for the
    // heroku-geo-buildpack (https://github.com/heroku/heroku-geo-buildpack) and allow a language like
    // Python to bind to libgdal-dev headers
    integration_test_with_config(
        "fixtures/project_file_with_empty_config",
        |config| {
            config
                .app_dir_preprocessor(|app_dir| {
                    set_install_config(&app_dir, [requested_package_config("libgdal-dev", false)]);
                    std::fs::write(app_dir.join("requirements.txt"), "GDAL==3.4.1").unwrap();
                })
                .buildpacks(vec![BuildpackReference::CurrentCrate, BuildpackReference::Other("heroku/python".to_string())]);
        },
        |ctx| {
            assert_contains!(ctx.pack_stdout, "Adding `libgdal-dev@3.4.1");
            assert_contains!(ctx.pack_stdout, "Collecting GDAL==3.4.1 (from -r requirements.txt (line 1))");
            assert_contains!(ctx.pack_stdout, "Successfully built GDAL");
            assert_contains!(ctx.pack_stdout, "Successfully installed GDAL-3.4.1");
        },
    );
}

#[test]
#[ignore = "integration test"]
fn vips_usage() {
    integration_test_with_config(
        "fixtures/project_file_with_empty_config",
        |config| {
            config.app_dir_preprocessor(|app_dir| {
                set_install_config(&app_dir, [requested_package_config("libvips-tools", false)]);
            });
        },
        |ctx| {
            assert_contains!(ctx.run_shell_command("vips --version").stdout, "vips-");
        },
    );
}

#[test]
#[ignore = "integration test"]
fn custom_repository_for_noble_distro() {
    if get_integration_test_builder().as_str() != "heroku/builder:24" {
        return;
    }
    integration_test("fixtures/custom_repository_noble", |ctx| {
        assert_contains!(ctx.pack_stdout, "https://repo.mongodb.org/apt/ubuntu noble/mongodb-org/8.0 [multiverse]");
        assert_contains!(ctx.pack_stdout, "Downloaded release file https://repo.mongodb.org/apt/ubuntu/dists/noble/mongodb-org/8.0/InRelease");
        assert_contains_match!(ctx.pack_stdout, r"Downloaded package index https://repo.mongodb.org/apt/ubuntu/dists/noble/mongodb-org/8.0/multiverse/binary-(amd|arm)64/Packages.gz");
        assert_contains!(ctx.pack_stdout, "Adding `mongodb-org-tools");
        assert_contains!(ctx.pack_stdout, "Adding `mongodb-org-shell");
    });
}

#[test]
#[ignore = "integration test"]
fn custom_deb_url_for_jammy_distro() {
    if get_integration_test_builder().as_str() != "heroku/builder:22" {
        return;
    }
    integration_test("fixtures/custom_deb_url_jammy", |ctx| {
        assert_not_contains!(ctx.pack_stdout, "Determining packages to install"); // this test only exercises downloads, so no package solving is performed
        assert_contains!(ctx.pack_stdout, "https://github.com/wkhtmltopdf/packaging/releases/download/0.12.6.1-2/wkhtmltox_0.12.6.1-2.jammy_amd64.deb");
        assert_contains!(ctx.run_shell_command("wkhtmltopdf --version").stdout, "wkhtmltopdf 0.12.6.1");
    });
}

const REBUILD_SEPARATOR: &str = "\
--------------------------------------------- REBUILD ---------------------------------------------";

#[bon::builder(finish_fn = assert)]
#[allow(clippy::needless_pass_by_value)]
fn create_build_snapshot(#[builder(start_fn, into)] build_output: String, #[builder(field)] custom_filters: Vec<(String, String)>, #[builder(into)] rebuild_output: Option<String>) {
    let mut filters = create_snapshot_filters();
    filters.extend(custom_filters);

    let snapshot_output = if let Some(rebuild_output) = rebuild_output { format!("{build_output}\n{REBUILD_SEPARATOR}\n{rebuild_output}") } else { build_output };

    with_settings!({
        omit_expression => true,
        prepend_module_to_snapshot => false,
        snapshot_path => snapshots_dir(),
        snapshot_suffix => snapshot_suffix(),
        filters => filters.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }, {
        assert_snapshot!(test_name(), snapshot_output);
    });
}

impl<S: create_build_snapshot_builder::State> CreateBuildSnapshotBuilder<S> {
    #[allow(dead_code)]
    fn filter(mut self, matcher: impl Into<String>, replacement: impl Into<String>) -> Self {
        self.custom_filters.push((matcher.into(), replacement.into()));
        self
    }
}

fn test_name() -> String {
    std::thread::current().name().expect("Test name should be available as the current thread name").to_string()
}

fn snapshot_suffix() -> String {
    let builder = get_integration_test_builder().replace(['/', ':'], "-");
    let arch = get_integration_test_arch();
    format!("{builder}_{arch}")
}

fn snapshots_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("snapshots")
}

#[allow(clippy::vec_init_then_push)]
fn create_snapshot_filters() -> Vec<(String, String)> {
    let mut filters: Vec<(&str, &str)> = vec![];

    // [pack] Filter out image name. e.g.;
    // - Saving libcnbtest_vtekdznblpdd...
    // - Successfully built image 'libcnbtest_prkmfnhkvvxu'
    filters.push((r"libcnbtest_[a-z]{12}", "<image-name>"));

    // [pack] Filter out "*** Images" output line. e.g.;
    // - *** Images (fbc060d7a40f):
    filters.push((r"Images \([a-z0-9]+\)", "Images (<random-hex>)"));

    // [pack] Filter out RESTORING section as output is non-deterministic
    filters.push((r"===> RESTORING\n(?:.*\n)*?===> BUILDING", "===> RESTORING\n<restoring-output>\n===> BUILDING"));

    // [pack] Filter out buildpack version in DETECTING section. e.g.;
    // - heroku/deb-packages  0.2.0
    filters.push((r"heroku/deb-packages(.*)\d+\.\d+\.\d+", "heroku/deb-packages$1<buildpack-version>"));

    // [bullet-stream] Timers from background progress completion. e.g.;
    // - (2m 22s)
    // - (10.9s)
    // - (< 0.3s)
    filters.push((r"(?:\(\d+m \d+s\)|\(\d+\.\d+s\)|\(< 0.\d+s\))", "(<time_elapsed>)"));

    // [bullet-stream] Dots from background activity
    filters.push((r" \.+ ", " ... "));

    // [bullet-stream] Timer from streamed command output completion. e.g.;
    // - Done (finished in 3m 29s)
    filters.push((r"- Done \(finished in .*\)", "- Done (finished in <time_elapsed>)"));

    // [deb-packages] Buildpack version in heading. e.g.;
    // - ## Heroku .deb Packages (v0.3.0)
    filters.push((r"Heroku \.deb Packages \(v\d+\.\d+\.\d+\)", "Heroku .deb Packages (v<buildpack-version>)"));

    // [deb-packages] SHA256 hashes in package index URLs. e.g.;
    // - by-hash/SHA256/abc123def456...
    filters.push((r"by-hash/SHA256/[0-9a-f]+", "by-hash/SHA256/<sha256-hash>"));

    // [deb-packages] Package index count which varies as repos are updated. e.g.;
    // - Indexed 73521 packages
    filters.push((r"Indexed \d+ packages", "Indexed <N> packages"));

    // [deb-packages] Release file cache state can vary between builds. e.g.;
    // - Restored release file from cache (http://...InRelease)
    // - Redownloaded release file http://...InRelease (Stored ETag did not match)
    filters.push((r"Restored release file from cache \((http://\S+)\)", "<RESTORED_OR_REDOWNLOADED> release file ($1)"));
    filters.push((r"Redownloaded release file (http://\S+) \(Stored ETag did not match\)", "<RESTORED_OR_REDOWNLOADED> release file ($1)"));

    // [pack] Cache layer hashes which are non-deterministic. e.g.;
    // - Adding cache layer 'heroku/deb-packages:0d7890e3ad86a9ebafdd72f64499f94aa395cd89...'
    // - Reusing cache layer 'heroku/deb-packages:0d7890e3ad86a9ebafdd72f64499f94aa395cd89...'
    filters.push((r"((?:Adding|Reusing) cache layer 'heroku/deb-packages:)[0-9a-f]+'", "${1}<layer-hash>'"));

    filters.into_iter().map(|(matcher, replacement)| (matcher.to_string(), replacement.to_string())).collect()
}

const DEFAULT_BUILDER: &str = "heroku/builder:24";

fn get_integration_test_builder() -> String {
    std::env::var("INTEGRATION_TEST_CNB_BUILDER").unwrap_or(DEFAULT_BUILDER.to_string())
}

const DEFAULT_ARCH: &str = "amd64";

fn get_integration_test_arch() -> String {
    std::env::var("INTEGRATION_TEST_CNB_ARCH").unwrap_or(DEFAULT_ARCH.to_string())
}

fn panic_unsupported_test_configuration() -> ! {
    panic!("Unsupported test configuration:\nINTEGRATION_TEST_CNB_BUILDER={}\nINTEGRATION_TEST_CNB_ARCH={}", get_integration_test_builder(), get_integration_test_arch());
}

fn integration_test(fixture: &str, test_body: fn(TestContext)) {
    integration_test_with_config(fixture, |_| {}, test_body);
}

fn integration_test_with_config(fixture: &str, with_config: fn(&mut BuildConfig), test_body: fn(TestContext)) {
    let builder = get_integration_test_builder();
    let app_dir = PathBuf::from("tests").join(fixture);

    let target_triple = match get_integration_test_arch().as_str() {
        "amd64" => "x86_64-unknown-linux-musl",
        "arm64" => "aarch64-unknown-linux-musl",
        _ => panic_unsupported_test_configuration(),
    };

    let mut build_config = BuildConfig::new(builder, app_dir);
    build_config.target_triple(target_triple);
    with_config(&mut build_config);

    TestRunner::default().build(build_config, test_body);
}

fn get_env_var(ctx: &TestContext, env_var_name: &str) -> String {
    ctx.run_shell_command(format!("echo -n ${env_var_name}")).stdout
}

fn read_package_config(ctx: &TestContext, package_config_path: &str) -> String {
    ctx.run_shell_command(format!("cat /layers/heroku_deb-packages/packages/{package_config_path}")).stdout
}

fn set_install_config<I>(app_dir: &Path, requested_packages: I)
where
    I: IntoIterator<Item = InlineTable>,
{
    update_project_toml(app_dir, |doc| {
        let root_config = doc
            .get_mut("com")
            .and_then(|item| item.as_table_like_mut())
            .and_then(|com| com.get_mut("heroku"))
            .and_then(|item| item.as_table_like_mut())
            .and_then(|heroku| heroku.get_mut("buildpacks"))
            .and_then(|item| item.as_table_like_mut())
            .and_then(|buildpacks| buildpacks.get_mut("deb-packages"))
            .and_then(|item| item.as_table_like_mut())
            .unwrap();
        root_config.insert("install", value(Array::from_iter(requested_packages)));
    });
}

fn requested_package_config(package: &str, skip_dependencies: bool) -> InlineTable {
    let mut requested_package = InlineTable::new();
    requested_package.insert("name", value(package).into_value().unwrap());
    requested_package.insert("skip_dependencies", value(skip_dependencies).into_value().unwrap());
    requested_package
}

fn update_project_toml(app_dir: &Path, update_fn: impl FnOnce(&mut DocumentMut)) {
    let project_toml = app_dir.join("project.toml");
    let contents = std::fs::read_to_string(&project_toml).unwrap();
    let mut doc = toml_edit::DocumentMut::from_str(&contents).unwrap();
    update_fn(&mut doc);
    std::fs::write(&project_toml, doc.to_string()).unwrap();
}
