# Heroku's `.deb` Packages Buildpack

[![CI][ci-badge]][ci-link] [![Registry][registry-badge]][registry-link]

`heroku/deb-packages` is a [Heroku Cloud Native Buildpack][heroku-cnbs] that adds support for installing Debian
packages required by an application that are not available in the build or run image used.

System dependencies on Debian distributions like Ubuntu are described by `<package-name>.deb` files. These are
typically installed using CLI tools such as `apt` or `dpkg`. This buildpack implements logic to install packages
from `.deb` files in a CNB-friendly manner that does not require root permissions or modifications to system files
that could invalidate how [CNB rebasing][cnb-rebase] functionality works.

> [!IMPORTANT]
> This is a [Cloud Native Buildpack][cnb], and is a component of the [Heroku Cloud Native Buildpacks][heroku-cnbs]
> project, which is in preview. If you are instead looking for the Heroku Apt Buildpack (for use on the Heroku
> platform), you may find it [here][classic-apt-buildpack].

This buildpack is compatible with the following environments:

| OS    | Arch  | Distro Name | Distro Version |
|-------|-------|-------------|----------------|
| linux | amd64 | Ubuntu      | 24.04          |
| linux | arm64 | Ubuntu      | 24.04          |
| linux | amd64 | Ubuntu      | 22.04          |

---

## Usage

> [!NOTE]
> Before getting started, ensure you have the pack CLI installed. Installation instructions are
> available [here][pack-install].

To include this buildpack in your application:

```shell
pack build my-app --builder heroku/builder:24 --buildpack heroku/deb-packages
```

And then run the image:

```shell
docker run --rm -it my-app
```

## Configuration

### `project.toml`

The configuration for this buildpack must be added to the project descriptor file (`project.toml`) at the root of your
project using the `com.heroku.buildpacks.deb-packages` table. The list of packages to install must be
specified there. See below for the [configuration schema](#schema) and an [example](#example).

#### Example

```toml
# _.schema-version is required for the project descriptor
[_]
schema-version = "0.2"

# buildpack configuration goes here
[com.heroku.buildpacks.deb-packages]
install = [
    # string version of a dependency to install
    "package-name",
    # inline-table version of a dependency to install
    { name = "package-name", skip_dependencies = true, force = true }
]

# one or more custom sources can be configured with the following:
[[com.heroku.buildpacks.deb-packages.sources]]
uri = "<url_of_debian_repository> (e.g.; http://archive.ubuntu.com/ubuntu)"
suites = ["<suite> (e.g.; jammy)"]
components = ["<component> (e.g.; main)"]
arch = ["<architecture> (e.g.; amd64 or arm64)"]
signed_by = """-----BEGIN PGP PUBLIC KEY BLOCK-----
<ASCII-armored GPG key>
-----END PGP PUBLIC KEY BLOCK-----
"""
```

#### Schema

- `com.heroku.buildpacks.deb-packages` *__([table][toml-table], optional)__*

  The root configuration for this buildpack.

    - `install` *__([array][toml-array], optional)__*

      A list of one or more packages to install. Each package can be specified in either of the following formats:

        - *__([string][toml-string])__*

          The name of the package to install.

      <p>&nbsp;&nbsp;&nbsp; <em><strong>OR</strong></em></p>

        - *__([inline-table][toml-inline-table])__*
            - `name` *__([string][toml-string], required)__*

              The name of the package to install.

            - `skip_dependencies` *__([boolean][toml-boolean], optional, default = false)__*

              If set to `true`, no attempt will be made to install any dependencies of the given package.

            - `force` *__([boolean][toml-boolean], optional, default = false)__*

              If set to `true`, the package will be installed even if it's already installed on the system.

    - `sources` *__([array_of_tables][toml-array-of-tables], optional)__*

        - `uri` *__([string][toml-string], required)__*

          The URI must specify the base of the Debian repository.

        - `suites` *__([array][toml-array] of [string][toml-string] values, required)__*

          One or more distribution suites from the Debian repository.

        - `components` *__([array][toml-array] of [string][toml-string] values, required)__*

          One or more components which specify different sections of distribution versions present in a suite.

        - `arch` *__([array][toml-array] of [string][toml-string] values, required)__*

          One or more supported architecture names. The supported architecture names are:
            - amd64
            - arm64

        - `signed_by` *__([string][toml-string], required)__*

          The GPG key required by the Debian repository in ASCII-armored format.

> [!TIP]
> Users of the [heroku-community/apt][classic-apt-buildpack] can migrate their Aptfile to the above configuration by
> adding a `project.toml` file with:
>
> ```toml
> [_]
> schema-version = "0.2"
>
> [com.heroku.buildpacks.deb-packages]
> install = [
>   # copy the contents of your Aptfile here, e.g.;
>   # "package-a",
>   # "package-b",
>   # "package-c"
> ]
> ```
>
> If your Aptfile contains a package name that uses wildcards (e.g.; `mysql-*`) this must be replaced with the full list
> of matching package names.

### Environment Variables

The following environment variables can be passed to the buildpack:

| Name           | Value               | Default | Description                                                                                        |
|----------------|---------------------|---------|----------------------------------------------------------------------------------------------------|
| `BP_LOG_LEVEL` | `INFO`,<br> `DEBUG` | `INFO`  | Configures the verbosity of buildpack output. The `DEBUG` level is a superset of the `INFO` level. |

## How it works

### Detection

This buildpack will pass detection if:

- A `project.toml` file is found at the root of the application source directory containing configuration under the
  `[com.heroku.buildpacks.deb-packages]` namespace.
- An `Aptfile` is found. This will not be used by this buildpack but details for how to migrate away from
  `Aptfile` configuration will be provided in the build phase if this file is present.

### Build

#### Step 1: Build the package index

Each supported distro is configured to download from the
following [Ubuntu repositories][about-ubuntu-repositories]:

- `main` - Canonical-supported free and open-source software.
- `universe` - Community-maintained free and open-source software.

These repositories comply with the [Debian Repository Format][debian-repository-format] so
building the list of packages involves:

- Downloading the [Release][release-file] file, validating its
  OpenPGP signature, and caching this in a [layer][cnb-layer] available at `build`.
- Finding and downloading the [Package Index][package-index-file] entry from the [Release][release-file] for the target
  architecture and caching this in a [layer][cnb-layer] available at `build`.
- Building an index of [Package Name][package-name-field] → ([Repository URI][debian-repository-uri],
  [Binary Package][debian-binary-package]) entries that can be used to lookup information about any packages requested
  for install.

#### Step 2: Determine the packages to install

For each package requested for install declared in the [buildpack configuration](#configuration):

- Lookup the [Binary Package][debian-binary-package] in the [Package Index](#step-1-build-the-package-index).
- Check if the requested package is already installed on the system
    - If it is already installed and the requested package is configured with `force = false`
        - Skip the package
- If the requested package is configured with `skip_dependencies = false`:
    - Add the latest version of the requested package.
    - Read the dependencies listed in the [Depends][binary-dependency-fields]
      and [Pre-Depends][binary-dependency-fields]
      from the [Binary Package][debian-binary-package].
    - For each dependency:
        - Recursively lookup the dependent package and follow the same steps outlined above until all transitive
          dependencies are added.
- If the requested package is configured with `skip_dependencies = true`:
    - Add the latest version of the requested package.

> [!NOTE]
> This buildpack is not meant to be a replacement for a fully-featured dependency manager like Apt. The simplistic
> dependency resolution strategy described above is for convenience, not accuracy. Any extra dependencies added are
> reported to the user during the build process so, if they aren't correct, you should disable the dependency resolution
> on a per-package basis with [configuration](#configuration) and explicitly list out each package you need installed.

#### Step 3: Install packages

For each package added after [determining the packages to install](#step-2-determine-the-packages-to-install):

- Download the [Binary Package][debian-binary-package] from the repository that contains it as
  a [Debian Archive][debian-archive].
- Extract the contents of the `data.tar` entry from the [Debian Archive][debian-archive] into a [layer][cnb-layer]
  available at `build` and `launch`.
- Rewrite any [pkg-config][package-config-file] files to use a `prefix` set to the layer directory of the installed
  package.
- Configure the following [layer environment variables][cnb-environment] to be available at both `build` and `launch`:

| Environment Variable | Appended Values                                                                                                  | Contents         |
|----------------------|------------------------------------------------------------------------------------------------------------------|------------------|
| `PATH`               | `/<layer_dir>/bin` <br> `/<layer_dir>/usr/bin` <br> `/<layer_dir>/usr/sbin`                                      | binaries         |
| `LD_LIBRARY_PATH`    | `/<layer_dir>/usr/lib/<arch>` <br> `/<layer_dir>/usr/lib` <br> `/<layer_dir>/lib/<arch>` <br> `/<layer_dir>/lib` | shared libraries |
| `LIBRARY_PATH`       | Same as `LD_LIBRARY_PATH`                                                                                        | static libraries |
| `INCLUDE_PATH`       | `/<layer_dir>/usr/include/<arch>` <br> `/<layer_dir>/usr/include`                                                | header files     |
| `CPATH`              | Same as `INCLUDE_PATH`                                                                                           | header files     |
| `CPPPATH`            | Same as `INCLUDE_PATH`                                                                                           | header files     |
| `PKG_CONFIG_PATH`    | `/<layer_dir>/usr/lib/<arch>/pkgconfig` <br> `/<layer_dir>/usr/lib/pkgconfig`                                    | pc files         |

## Contributing

Issues and pull requests are welcome. See our [contributing guidelines](./CONTRIBUTING.md) if you would like to help.

[about-ubuntu-repositories]: https://help.ubuntu.com/community/Repositories/Ubuntu

[binary-dependency-fields]: https://www.debian.org/doc/debian-policy/ch-relationships.html#binary-dependencies-depends-recommends-suggests-enhances-pre-depends

[ci-badge]: https://github.com/heroku/buildpacks-deb-packages/actions/workflows/ci.yml/badge.svg

[ci-link]: https://github.com/heroku/buildpacks-deb-packages/actions/workflows/ci.yml

[classic-apt-buildpack]: https://github.com/heroku/heroku-buildpack-apt

[cnb]: https://buildpacks.io/

[cnb-environment]: https://github.com/buildpacks/spec/blob/main/buildpack.md#environment

[cnb-layer]: https://github.com/buildpacks/spec/blob/main/buildpack.md#layer-types

[cnb-rebase]: https://buildpacks.io/docs/for-app-developers/concepts/rebase/

[debian-archive]: https://www.man7.org/linux/man-pages/man5/deb.5.html

[debian-binary-package]: https://www.debian.org/doc/debian-policy/ch-binary.html

[debian-repository-format]: https://wiki.debian.org/DebianRepository/Format

[debian-repository-uri]: https://wiki.debian.org/DebianRepository/Format#Overview

[heroku-cnbs]: https://github.com/heroku/buildpacks

[pack-install]: https://buildpacks.io/docs/for-platform-operators/how-to/integrate-ci/pack/

[package-config-file]: https://manpages.ubuntu.com/manpages/noble/en/man5/pc.5.html

[package-index-file]: https://wiki.debian.org/DebianRepository/Format#A.22Packages.22_Indices

[package-name-field]: https://www.debian.org/doc/debian-policy/ch-controlfields.html#package

[project-descriptor]: https://buildpacks.io/docs/reference/config/project-descriptor/

[registry-badge]: https://img.shields.io/badge/dynamic/json?url=https://registry.buildpacks.io/api/v1/buildpacks/heroku/deb-packages&label=version&query=$.latest.version&color=DF0A6B&logo=data:image/png;base64,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&labelColor=white

[registry-link]: https://registry.buildpacks.io/buildpacks/heroku/deb-packages

[release-file]: https://wiki.debian.org/DebianRepository/Format#A.22Release.22_files

[toml-array]: https://toml.io/en/v1.0.0#array

[toml-boolean]: https://toml.io/en/v1.0.0#boolean

[toml-inline-table]: https://toml.io/en/v1.0.0#inline-table

[toml-string]: https://toml.io/en/v1.0.0#string

[toml-table]: https://toml.io/en/v1.0.0#table

[toml-array-of-tables]: https://toml.io/en/v1.0.0#array-of-tables
