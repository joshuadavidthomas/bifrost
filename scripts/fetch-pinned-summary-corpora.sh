#!/usr/bin/env bash
# Fetch the pinned external procedure-summary corpora and run the foundry join.
#
# The corpora are third-party content and are NOT vendored into this
# repository. Only small verbatim slices live under
# crates/bifrost-semantic-packs/testdata/summary-corpora/ so translator tests
# run offline.
#
#   CodeQL Models-as-Data  github/codeql        MIT
#   Joern flow semantics   joernio/joern        Apache-2.0
#
# semantic-packs/summary-corpora/pins.json is the single source of truth for
# the revision, the archive URL, its sha256, and the member path this script
# extracts. Bump a pin there and both this script and the join tool follow.
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "usage: $0 WORK_DIR [REPORT_JSON]" >&2
  exit 2
fi

repository_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
work_dir=$1
report_path=${2:-${work_dir}/summary-corpus-join.json}
pins_path="${repository_root}/semantic-packs/summary-corpora/pins.json"
download_dir="${work_dir}/downloads"
extract_dir="${work_dir}/corpora"

mkdir -p "${download_dir}" "${extract_dir}"

# One node reader keeps the pins file the only place a revision is written.
read_pin() {
  node --input-type=module -e "
    import { readFileSync } from 'node:fs';
    const pins = JSON.parse(readFileSync('${pins_path}', 'utf8'));
    const record = pins.corpora.find((corpus) => corpus.corpus === '$1');
    if (!record) {
      throw new Error('pins.json declares no corpus $1');
    }
    process.stdout.write(String($2));
  "
}

fetch_corpus() {
  corpus=$1
  url=$(read_pin "${corpus}" "record.archive.url")
  sha256=$(read_pin "${corpus}" "record.archive.sha256")
  member=$(read_pin "${corpus}" "record.archive.member")
  archive="${download_dir}/${corpus}.tar.gz"

  curl --fail --location --silent --show-error --retry 5 --retry-all-errors \
    --retry-delay 5 --connect-timeout 30 --output "${archive}" "${url}"
  echo "${sha256}  ${archive}" | shasum -a 256 --check --status \
    || { echo "error: ${corpus} archive does not match its pinned sha256" >&2; exit 1; }
  tar -C "${extract_dir}" -xzf "${archive}" "${member}"
  echo "${extract_dir}/${member}"
}

codeql_models=$(fetch_corpus codeql)
joern_source=$(fetch_corpus joern)

cargo run --locked --release --features release-tooling \
  -p brokk-bifrost-semantic-packs --bin bifrost-semantic-pack -- summary-corpus-join \
  "${pins_path}" "${codeql_models}" "${joern_source}" "${report_path}"
