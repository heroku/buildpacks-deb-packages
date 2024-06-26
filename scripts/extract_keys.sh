#!/usr/bin/env bash

# Use this script to extract the PGP certificate stored in the Apt keyring as ASCII-armored text.
#
# This will execute for each target architecture and distro specified in this project's buildpack.toml which might
# be overkill since it's unlikely the key will be different between architecture for the same distro. Still, that might
# be helpful in some cases and the output files are annotated with an MD5 signatures so it's easy to see which ones
# contain differences.

set -euo pipefail

base_dir=$(realpath "$(dirname "${BASH_SOURCE[0]}")/..")

for target in $(yq --exit-status --output-format json --indent 0 '.targets[]' "$base_dir/buildpack.toml"); do
  os=$(echo "$target" | yq --unwrapScalar --output-format json '.os')
  arch=$(echo "$target" | yq --unwrapScalar --output-format json '.arch')
  for distro in $(echo "$target" | yq --output-format json --indent 0 '.distros[]'); do
    name=$(echo "$distro" | yq --unwrapScalar --output-format json '.name')
    version=$(echo "$distro" | yq --unwrapScalar --output-format json '.version')

    platform="$os/$arch"
    docker_image="$name:$version"
    volume_external="$base_dir/keys"
    volume_internal="/extracted_keys"
    key_output="${name}_${version}_${os}_${arch}"

    echo "Extracting keyring from $docker_image ($platform)..."
    docker run \
      --volume "$volume_external:$volume_internal:rw" \
      --platform "$platform" \
      --rm -it "$docker_image" \
      bash -c "apt update && apt install -y gnupg && apt-key export 'ftpmaster@ubuntu.com' > /$volume_internal/$key_output"

    checksum=$(md5sum "$volume_external/$key_output" | cut -d ' ' -f 1)
    mv "$volume_external/$key_output" "$volume_external/$key_output.$checksum.asc"

    echo "Saved to $volume_external/$key_output.$checksum.asc"
  done
done
