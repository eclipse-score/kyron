#!/usr/bin/env bash

# Exit immediately if any command fails
set -e

echo "----------- CURRENTLY VIRTUALIZATION IS DISABLED FOR QNX8. TO ENABLE SET `disable = False` flag in virtualization/BUILD -----------"

# Check if SCORE_QNX_USER and SCORE_QNX_PASSWORD are set
if [[ -z "${SCORE_QNX_USER}" || -z "${SCORE_QNX_PASSWORD}" ]]; then
  # If either is missing, check for .netrc
  if [[ ! -f "$HOME/.netrc" ]]; then
    echo "Error: Either SCORE_QNX_USER/SCORE_QNX_PASSWORD must be set, or $HOME/.netrc must exist."
    exit 1
  fi
fi

echo "Note: If it fails with 'Error downloading [https://www.qnx.com/download/download/79858/installation.tgz' access this link from webbrowser and accept the license agreement."

bazel run  --config=build_qnx8 --credential_helper=*.qnx.com=%workspace%/scripts/internal/qnx_creds.py //virtualization:virtualization_run_qemu
