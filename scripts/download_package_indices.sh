#!/usr/bin/env bash

# This script is useful for checking out the contents of the package indices for a given distribution. Pair it with a
# tool like ripgrep and it's fairly easy to investigate package information.

set -euo pipefail

args=($*)
error_with_usage() {
  message=${1:-"Error!"}
  echo "$message"
  echo
  echo "Usage:"
  echo "${BASH_SOURCE[0]} <jammy | noble> <linux/arm64 | linux/amd64>"
  echo
  echo "Given:"
  echo "${BASH_SOURCE[0]} ${args[0]:-"<missing arg>"} ${args[1]:-"<missing arg>"}"
  exit 1
}

if [ $# -lt 2 ]; then
  error_with_usage "Not enough arguments!" "$}"
fi

base_dir=$(realpath "$(dirname "${BASH_SOURCE[0]}")/..")

case "$1" in
  "jammy")
    docker_image="ubuntu:22.04"
    ;;
  "noble")
    docker_image="ubuntu:24.04"
    ;;
  *)
    error_with_usage "Expected argument #1 to be either jammy or noble but it was $1!"
    ;;
esac

case "$2" in
  "linux/arm64" | "linux/amd64")
    platform="$2"
    ;;
  *)
    error_with_usage "Expected argument #2 to be either linux/arm64 or linux/amd64 but it was $2!"
    ;;
esac

external_output_dir="$base_dir/target/tmp/package_indices/${docker_image//:/_}_${platform//\//_}"
internal_output_dir="/package_indices"

external_script_dir=$(mktemp -d)
internal_script_dir="/script"

internal_download_dir="/tmp/downloads"

rm -rf "${external_output_dir:?}"
mkdir -p "$external_output_dir"
cat << EOF > "$external_script_dir/run.sh"
#!/usr/bin/env bash
set -euo pipefail
set -o xtrace
mkdir -p $internal_download_dir
apt -oDir::State::lists=$internal_download_dir update
apt -oDir::State::lists=$internal_download_dir install -y lz4
find $internal_download_dir -maxdepth 1 -type f \
  | grep -E '.*_(main|universe)_.*_Packages.lz4' \
  | xargs basename -s .lz4 \
  | xargs -I % lz4 -d $internal_download_dir/%.lz4 $internal_output_dir/%.txt
EOF
chmod +x "$external_script_dir/run.sh"

echo "Extracting package indices from $docker_image..."
docker run \
  --volume "$external_output_dir:$internal_output_dir:rw" \
  --volume "$external_script_dir:$internal_script_dir:rw" \
  --platform "$platform" \
  --rm -it "$docker_image" \
  "$internal_script_dir/run.sh"
echo "Saved to $external_output_dir"
