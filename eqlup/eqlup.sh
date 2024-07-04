#!/bin/bash

USERNAME="iankressin"
REPO_NAME="eql"
REPO_URL="https://github.com/{$USERNAME}/${REPO_NAME}"
REPO_API_URL="https://api.github.com/repos/${USERNAME}/${REPO_NAME}"

LINUX_ASSET="eql"
MAC_ASSET="eql"

get_latest_release_tag() {
    LATEST_RELEASE_TAG=$(curl -s "${REPO_API_URL}/releases/latest" | grep -Po '"tag_name": "\K.*?(?=")')
}

initial_message() {
    echo "
    ███████╗ ██████╗ ██╗     ██╗   ██╗██████╗ 
    ██╔════╝██╔═══██╗██║     ██║   ██║██╔══██╗
    █████╗  ██║   ██║██║     ██║   ██║██████╔╝
    ██╔══╝  ██║▄▄ ██║██║     ██║   ██║██╔═══╝ 
    ███████╗╚██████╔╝███████╗╚██████╔╝██║     
    ╚══════╝ ╚══▀▀═╝ ╚══════╝ ╚═════╝ ╚═╝

        ((( The eql version manager )))
    "

    echo "[INFO] Installing the lastest version of eql: $LATEST_RELEASE_TAG"
}

detect_os() {
    if [ "$OSTYPE" == "linux-gnu" ]; then
        ASSET_NAME=$LINUX_ASSET
        echo "[INFO] Linux detected"
    elif [ "$OSTYPE" == "darwin"* ]; then
        ASSET_NAME=$MAC_ASSET
        echo "[INFO] MacOS detected"
    elif [ "$OSTYPE" == "cygwin" ]; then
        echo "[INFO] On Windows, download the executable from the link below:"
        echo "{ $REPO_URL }/releases/latest"
        exit 1
    else
        echo "[INFO] Unsupported OS"
        exit 1
    fi
}

download_asset() {
    echo "[INFO] Downloading asset"
    curl -L -o eql-release "${REPO_URL}/releases/download/${LATEST_RELEASE_TAG}/${ASSET_NAME}"
    echo "[INFO] Asset downloaded"
}

move_to_bin() {
    echo "[INFO] Moving to /usr/local/bin"
    sudo mv eql-release /usr/local/bin/eql
    chmod +x /usr/local/bin/eql
    echo "[INFO] Installed to /usr/local/bin/eql"
}

cleanup() {
    rm -rf latest latest.zip
    echo "[INFO] Cleaned up"
}

remove_old_version() {
    echo "[INFO] Removing old version of eql"
    sudo rm -f /usr/local/bin/eql
    echo "[INFO] Old version removed "
}

final_message() {
    echo "---------------------- Installation complete ----------------------"
    echo ">>> Run 'eql --help' to get started"
}

main() {
    get_latest_release_tag
    initial_message
    remove_old_version
    detect_os
    download_asset
    move_to_bin
    cleanup
    final_message
}

main
