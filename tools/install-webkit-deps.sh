#!/usr/bin/env bash
# Keep the runner's signed Ubuntu sources and mirror failover intact. The caller
# bounds the whole setup step; per-acquisition limits alone do not bound DNS or
# the sum of downloads. See docs/engineering/webkit-ci-setup.md.
set -euo pipefail
apt_options=(-o Acquire::Retries=1 -o Acquire::http::Timeout=15 -o Acquire::https::Timeout=15)
sudo apt-get "${apt_options[@]}" -o APT::Update::Error-Mode=any update -q

# The driver was renamed in Ubuntu 26.04. Choose from refreshed package metadata,
# not by retrying a failed installation (which may be a network/signature error).
driver=
for package in webkitgtk-webdriver webkit2gtk-driver; do
  policy=$(LC_ALL=C apt-cache policy "$package")
  candidate=$(awk '$1 == "Candidate:" { print $2; exit }' <<< "$policy")
  if [ -n "$candidate" ] && [ "$candidate" != '(none)' ]; then
    driver=$package
    break
  fi
done
if [ -z "$driver" ]; then
  echo 'Neither Ubuntu WebKit WebDriver package has an installation candidate.' >&2
  exit 1
fi
sudo apt-get "${apt_options[@]}" install -yq --no-install-recommends \
  "$driver" libwebkit2gtk-4.1-0 libgl1-mesa-dri xvfb xauth dbus-x11
