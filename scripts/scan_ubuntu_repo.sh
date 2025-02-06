#!/bin/bash

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORK_DIR="${SCRIPT_DIR}/ubuntu_packages_analysis"
RESULTS_DIR="${WORK_DIR}/results"

# Create working directories
mkdir -p "${WORK_DIR}/tmp"
mkdir -p "${RESULTS_DIR}"

# Cleanup function for temporary files
cleanup() {
    rm -rf "${WORK_DIR}/tmp/*"
    echo "Cleaned up temporary files"
}
trap cleanup EXIT

REPO_BASE="http://archive.ubuntu.com/ubuntu/pool/universe"

# Function to analyze deb package
analyze_package() {
    local pkg_url=$1
    local pkg_name=$(basename "$pkg_url")
    local tmp_path="${WORK_DIR}/tmp/${pkg_name}"
    
    echo "Analyzing $pkg_name..."
    curl -s -o "$tmp_path" "$pkg_url"
    
    cd "${WORK_DIR}/tmp"
    if ar x "$pkg_name" control.tar.gz >/dev/null 2>&1; then
        scripts=$(tar tzf control.tar.gz 2>/dev/null | grep -E '(postinst|preinst|postrm|prerm)' | tr '\n' ' ')
        echo "$pkg_url | ${scripts:-none}" >> "${RESULTS_DIR}/packages_with_scripts.txt"
    fi
    
    rm -f "$pkg_name" control.tar.gz
}

# Recursive function to process directories
process_directory() {
    local current_url=$1
    echo "Processing directory: $current_url"
    
    curl -s "$current_url" | grep -o 'href="[^"]*"' | cut -d'"' -f2 | while read -r entry; do
        [[ "$entry" == "../" ]] && continue
        
        local full_url="${current_url}${entry}"
        
        if [[ "$entry" =~ .+/$ ]]; then
            process_directory "$full_url"
        elif [[ "$entry" =~ \.deb$ ]]; then
            analyze_package "$full_url"
        fi
    done
}

echo "Working directory: $WORK_DIR"
echo "Results will be saved to: ${RESULTS_DIR}/packages_with_scripts.txt"

# Start processing from base directory
process_directory "${REPO_BASE}/"

echo "Analysis complete. Results saved to: ${RESULTS_DIR}/packages_with_scripts.txt"
