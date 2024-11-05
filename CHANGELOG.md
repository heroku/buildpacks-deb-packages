# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Support the `force` option when declaring a package to install so the user can control if, when the package is already
  on
  the system, it should be skipped or forcibly installed.

### Changed

- Updated the buildpack display name and keywords to be more consistent with our other
  CNBs. ([#59](https://github.com/heroku/buildpacks-deb-packages/pull/59))
- Modified the buildpack output format to align with our other
  CNBs. ([#60](https://github.com/heroku/buildpacks-deb-packages/pull/60))

## [0.0.1] - 2024-10-10

### Added

- Initial release.

[unreleased]: https://github.com/heroku/buildpacks-deb-packages/compare/v0.0.1...HEAD

[0.0.1]: https://github.com/heroku/buildpacks-deb-packages/releases/tag/v0.0.1
