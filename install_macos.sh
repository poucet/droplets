#!/bin/bash

# Simply Droplets - macOS Installation Script for Ableton Live
# This script will install the VST3 plugin to the correct location for Ableton Live

set -e

echo "🎵 Simply Droplets - macOS Installation Script"
echo "==============================================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Check if we're on macOS
if [[ "$OSTYPE" != "darwin"* ]]; then
    echo -e "${RED}❌ This script is for macOS only${NC}"
    exit 1
fi

# Plugin paths for macOS
VST3_USER_PATH="$HOME/Library/Audio/Plug-Ins/VST3"
VST3_SYSTEM_PATH="/Library/Audio/Plug-Ins/VST3"
CLAP_USER_PATH="$HOME/Library/Audio/Plug-Ins/CLAP"

# Source plugin file
SOURCE_VST3="target/bundled/simply_droplets.vst3"
SOURCE_CLAP="target/bundled/simply_droplets.clap"

echo "🔍 Checking for plugin files..."

# Check if plugin files exist
if [ ! -d "$SOURCE_VST3" ]; then
    echo -e "${RED}❌ VST3 plugin not found at $SOURCE_VST3${NC}"
    echo -e "${YELLOW}💡 Run this first: cargo run --manifest-path xtask/Cargo.toml -- bundle simply_droplets${NC}"
    exit 1
fi

echo -e "${GREEN}✅ Found VST3 plugin${NC}"

if [ -d "$SOURCE_CLAP" ]; then
    echo -e "${GREEN}✅ Found CLAP plugin${NC}"
    INSTALL_CLAP=true
else
    echo -e "${YELLOW}⚠️  CLAP plugin not found (optional)${NC}"
    INSTALL_CLAP=false
fi

# Create VST3 directory if it doesn't exist
echo "📁 Creating plugin directories..."
mkdir -p "$VST3_USER_PATH"
if [ "$INSTALL_CLAP" = true ]; then
    mkdir -p "$CLAP_USER_PATH"
fi

# Function to install VST3
install_vst3() {
    echo "🔧 Installing VST3 plugin..."
    
    # Remove existing installation if present
    if [ -d "$VST3_USER_PATH/simply_droplets.vst3" ]; then
        echo "🗑️  Removing existing installation..."
        rm -rf "$VST3_USER_PATH/simply_droplets.vst3"
    fi
    
    # Copy the plugin
    cp -R "$SOURCE_VST3" "$VST3_USER_PATH/"
    
    if [ -d "$VST3_USER_PATH/simply_droplets.vst3" ]; then
        echo -e "${GREEN}✅ VST3 installed successfully to: $VST3_USER_PATH${NC}"
    else
        echo -e "${RED}❌ VST3 installation failed${NC}"
        exit 1
    fi
}

# Function to install CLAP
install_clap() {
    if [ "$INSTALL_CLAP" = true ]; then
        echo "🔧 Installing CLAP plugin..."
        
        # Remove existing installation if present
        if [ -f "$CLAP_USER_PATH/simply_droplets.clap" ]; then
            echo "🗑️  Removing existing CLAP installation..."
            rm -f "$CLAP_USER_PATH/simply_droplets.clap"
        fi
        
        # Copy the plugin
        cp "$SOURCE_CLAP" "$CLAP_USER_PATH/"
        
        if [ -f "$CLAP_USER_PATH/simply_droplets.clap" ]; then
            echo -e "${GREEN}✅ CLAP installed successfully to: $CLAP_USER_PATH${NC}"
        else
            echo -e "${RED}❌ CLAP installation failed${NC}"
        fi
    fi
}

# Install plugins
install_vst3
install_clap

echo ""
echo -e "${BLUE}🎹 Ableton Live Setup Instructions:${NC}"
echo "1. Open Ableton Live"
echo "2. Go to Live > Preferences > Plug-ins"
echo "3. Make sure 'Use VST3 Plug-in System Folders' is enabled"
echo "4. Click 'Rescan' to refresh the plugin list"
echo "5. Look for 'Simply Droplets' in your Audio Effects"
echo ""
echo -e "${YELLOW}📍 Plugin installed to:${NC}"
echo "   VST3: $VST3_USER_PATH/simply_droplets.vst3"
if [ "$INSTALL_CLAP" = true ]; then
    echo "   CLAP: $CLAP_USER_PATH/simply_droplets.clap"
fi
echo ""
echo -e "${GREEN}🎉 Installation complete!${NC}"
echo ""
echo -e "${BLUE}🐛 Troubleshooting:${NC}"
echo "• If plugin doesn't appear: Restart Ableton Live completely"
echo "• Check Console.app for any error messages"
echo "• Verify plugin architecture matches your Ableton Live (Intel/Apple Silicon)"
echo ""
echo -e "${YELLOW}⚠️  Note about UI:${NC}"
echo "The React UI may have compatibility issues on macOS due to webview limitations."
echo "The audio processing will work regardless - you can still use parameter automation!"