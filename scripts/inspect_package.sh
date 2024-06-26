#!/usr/bin/env bash

# This script is useful for inspecting the contents of a debian package for a given distribution.

set -euo pipefail

args=($*)
error_with_usage() {
  message=${1:-"Error!"}
  echo "$message"
  echo
  echo "Usage:"
  echo "${BASH_SOURCE[0]} <package_name> <jammy | noble> <linux/arm64 | linux/amd64>"
  echo
  echo "Given:"
  echo "${BASH_SOURCE[0]} ${args[0]:-"<missing arg>"} ${args[1]:-"<missing arg>"} ${args[2]:-"<missing arg>"}"
  exit 1
}

if [ $# -lt 3 ]; then
  error_with_usage "Not enough arguments!" "$}"
fi

base_dir=$(realpath "$(dirname "${BASH_SOURCE[0]}")/..")

package_name="$1"

case "$2" in
  "jammy")
    docker_image="ubuntu:22.04"
    ;;
  "noble")
    docker_image="ubuntu:24.04"
    ;;
  *)
    error_with_usage "Expected argument #1 to be either jammy or noble but it was $2!"
    ;;
esac

case "$3" in
  "linux/arm64" | "linux/amd64")
    platform="$3"
    ;;
  *)
    error_with_usage "Expected argument #2 to be either linux/arm64 or linux/amd64 but it was $3!"
    ;;
esac

external_output_dir="$base_dir/target/tmp/debian_packages/${docker_image//:/_}_${platform//\//_}"
internal_output_dir="/package"

external_script_dir=$(mktemp -d)
internal_script_dir="/script"

internal_download_dir="/tmp/downloads"
internal_extract_dir="$internal_output_dir/$package_name"

rm -rf "${external_output_dir:?}/${package_name:?}"
mkdir -p "$external_output_dir"
cat << EOF > "$external_script_dir/run.sh"
#!/usr/bin/env bash
set -euo pipefail
set -o xtrace
mkdir -p $internal_download_dir && mkdir -p $internal_extract_dir
apt update && apt install -y binutils zstd
apt-get -oDir::cache=$internal_download_dir -y -d install $package_name
cd $internal_extract_dir
ar xv $internal_download_dir/archives/${package_name}_*.deb
mkdir ./control && tar -C ./control -xvf control.tar.* && rm control.tar.*
mkdir ./data && tar -C ./data -xvf data.tar.* && rm data.tar.*
EOF
chmod +x "$external_script_dir/run.sh"

echo "Downloading debian package $package_name from $docker_image..."
docker run \
  --volume "$external_output_dir:$internal_output_dir:rw" \
  --volume "$external_script_dir:$internal_script_dir:rw" \
  --platform "$platform" \
  --rm -it "$docker_image" \
  "$internal_script_dir/run.sh"
echo "Saved to $external_output_dir/$package_name"
