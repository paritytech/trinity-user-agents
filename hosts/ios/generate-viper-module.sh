#!/bin/bash

# Script to generate VIPER modules using SwiftGen-style templates
# Usage: ./generate-viper-module.sh ModuleName
#
# This script replaces the legacy generamba-module.sh script
# and uses SwiftGen-compatible Stencil templates for code generation.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [ -z "$1" ]; then
    echo "Error: Module name is required" >&2
    echo "Usage: $0 ModuleName" >&2
    exit 1
fi

MODULE_NAME="$1"

# Validate module name (should start with uppercase letter and contain only alphanumeric)
if ! [[ "$MODULE_NAME" =~ ^[A-Z][a-zA-Z0-9]*$ ]]; then
    echo "Error: Module name must start with an uppercase letter and contain only alphanumeric characters" >&2
    exit 1
fi

TEMPLATES_DIR="$SCRIPT_DIR/swiftgen-templates/viper"
MODULES_DIR="$SCRIPT_DIR/polkadot-app/Modules"
MODULE_DIR="$MODULES_DIR/$MODULE_NAME"
POLKADOT_UI_DIR="$SCRIPT_DIR/Packages/PolkadotUI/Sources/Modules"
POLKADOT_UI_MODULE_DIR="$POLKADOT_UI_DIR/$MODULE_NAME"

# Check if module already exists
if [ -d "$MODULE_DIR" ]; then
    echo "Error: Module '$MODULE_NAME' already exists at $MODULE_DIR" >&2
    exit 1
fi

# Check if templates directory exists
if [ ! -d "$TEMPLATES_DIR" ]; then
    echo "Error: Templates directory not found at $TEMPLATES_DIR" >&2
    exit 1
fi

# Check if modules directory exists
if [ ! -d "$MODULES_DIR" ]; then
    echo "Error: Modules directory not found at $MODULES_DIR" >&2
    exit 1
fi

# Check if PolkadotUI directory exists
if [ ! -d "$POLKADOT_UI_DIR" ]; then
    echo "Error: PolkadotUI Modules directory not found at $POLKADOT_UI_DIR" >&2
    exit 1
fi

# Function to process template using sed (simple template substitution)
process_template() {
    local template_file="$1"
    local output_file="$2"
    
    if [ ! -f "$template_file" ]; then
        echo "Error: Template file not found: $template_file" >&2
        return 1
    fi
    
    # Use sed to replace placeholders in template
    # Escape special characters in module name for sed
    local escaped_module_name=$(echo "$MODULE_NAME" | sed 's/[[\.*^$()+?{|]/\\&/g')
    
    sed -e "s/{{moduleName}}/$escaped_module_name/g" \
        "$template_file" > "$output_file"
    
    # Verify the file was created
    if [ ! -f "$output_file" ]; then
        echo "Error: Failed to create $output_file" >&2
        return 1
    fi
}

# Create module directories
mkdir -p "$MODULE_DIR"
mkdir -p "$POLKADOT_UI_MODULE_DIR"

# Function to get output filename and directory for a template
get_output_path() {
    local template_key="$1"
    case "$template_key" in
        interactor)     echo "$MODULE_DIR|${MODULE_NAME}Interactor.swift" ;;
        presenter)      echo "$MODULE_DIR|${MODULE_NAME}Presenter.swift" ;;
        protocols)      echo "$MODULE_DIR|${MODULE_NAME}Protocols.swift" ;;
        viewcontroller) echo "$MODULE_DIR|${MODULE_NAME}ViewController.swift" ;;
        viewfactory)    echo "$MODULE_DIR|${MODULE_NAME}ViewFactory.swift" ;;
        wireframe)      echo "$MODULE_DIR|${MODULE_NAME}Wireframe.swift" ;;
        viewlayout)     echo "$POLKADOT_UI_MODULE_DIR|${MODULE_NAME}ViewLayout.swift" ;;
        viewmodel)      echo "$POLKADOT_UI_MODULE_DIR|${MODULE_NAME}ViewModel.swift" ;;
        *)              echo "|" ;;
    esac
}

# Generate all VIPER component files
echo "Generating VIPER module: $MODULE_NAME"
echo "Output directory: $MODULE_DIR"
echo ""

# List of template files to process (bash 3.2 compatible)
TEMPLATE_KEYS="interactor presenter protocols viewcontroller viewfactory wireframe viewlayout viewmodel"
GENERATED_FILES=""

# Process each template
for template_key in $TEMPLATE_KEYS; do
    template_file="$TEMPLATES_DIR/${template_key}.stencil"
    output_path=$(get_output_path "$template_key")
    output_dir=$(echo "$output_path" | cut -d'|' -f1)
    output_filename=$(echo "$output_path" | cut -d'|' -f2)
    output_file="$output_dir/$output_filename"
    
    if process_template "$template_file" "$output_file"; then
        echo "  ✓ Generated $output_filename"
        GENERATED_FILES="$GENERATED_FILES $output_filename"
    else
        echo "  ✗ Failed to generate $output_filename" >&2
        exit 1
    fi
done

echo ""
echo "✓ Successfully generated VIPER module '$MODULE_NAME'"
echo ""
echo "Generated files in module directory ($MODULE_DIR):"
for output_file in $GENERATED_FILES; do
    case "$output_file" in
        *ViewLayout.swift|*ViewModel.swift) ;;
        *)                echo "  - $output_file" ;;
    esac
done
echo ""
echo "Generated files in PolkadotUI package ($POLKADOT_UI_MODULE_DIR):"
for output_file in $GENERATED_FILES; do
    case "$output_file" in
        *ViewLayout.swift) echo "  - $output_file" ;;
        *ViewModel.swift)  echo "  - $output_file" ;;
        *) ;;
    esac
done
echo ""
echo "Next steps:"
echo "  1. Add the generated files to your Xcode project"
echo "  2. Implement the business logic in each component"
echo "  3. Customize the SwiftUI ViewLayout and ViewModel in PolkadotUI package"

