# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.3] - 2024-12-05

### Changed

- `PackageNotFound` errors now return a list of suggested package names.
  ([#69](https://github.com/heroku/buildpacks-deb-packages/pull/69))

## [0.0.2] - 2024-11-06

### Added

- Support the `force` option when declaring a package to install so the user can control if, when the package is already
  on the system, it should be skipped or forcibly installed. ([#66](https://github.com/heroku/buildpacks-deb-packages/pull/66))
- Support for `BP_LOG_LEVEL` environment variable to control verbosity of buildpack output.
  ([#60](https://github.com/heroku/buildpacks-deb-packages/pull/60))

### Changed

- Updated the buildpack display name and keywords to be more consistent with our other
  CNBs. ([#59](https://github.com/heroku/buildpacks-deb-packages/pull/59))
- Modified the buildpack output format to align with our other
  CNBs. ([#60](https://github.com/heroku/buildpacks-deb-packages/pull/60))

## [0.0.1] - 2024-10-10

### Added

- Initial release.

[unreleased]: https://github.com/heroku/buildpacks-deb-packages/compare/v0.0.3...HEAD
[0.0.3]: https://github.com/heroku/buildpacks-deb-packages/compare/v0.0.2...v0.0.3
[0.0.2]: https://github.com/heroku/buildpacks-deb-packages/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/heroku/buildpacks-deb-packages/releases/tag/v0.0.1
