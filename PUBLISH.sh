#!/bin/bash
# PyPI Publishing Quick Reference Script
# Usage: bash PUBLISH.sh [testpypi|pypi]

set -e

PACKAGE_NAME="fastapi-data-access-factory"
VERSION="0.1.0"
DIST_DIR="dist"

echo "╔════════════════════════════════════════════════════════════╗"
echo "║  PyPI Publishing Script for $PACKAGE_NAME  ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""

# Check arguments
if [ $# -eq 0 ]; then
    echo "Usage: bash $0 [testpypi|pypi|clean|verify]"
    echo ""
    echo "Commands:"
    echo "  testpypi    - Upload to TestPyPI for testing"
    echo "  pypi        - Upload to production PyPI"
    echo "  clean       - Clean build artifacts"
    echo "  verify      - Verify package contents"
    echo "  build       - Build wheel and source dist"
    exit 1
fi

COMMAND=$1

case $COMMAND in
    build)
        echo "🔨 Building package..."
        rm -rf build/ dist/ *.egg-info
        uv build
        echo "✅ Build complete!"
        ls -lh dist/
        ;;
    
    verify)
        echo "🔍 Verifying package contents..."
        echo ""
        echo "Wheel contents:"
        unzip -l dist/${PACKAGE_NAME}-${VERSION}-py3-none-any.whl | head -20
        echo ""
        echo "Source dist contents:"
        tar -tzf dist/${PACKAGE_NAME}-${VERSION}.tar.gz | head -20
        ;;
    
    clean)
        echo "🧹 Cleaning build artifacts..."
        rm -rf build/ dist/ .eggs/ *.egg-info .ruff_cache .mypy_cache .pytest_cache
        find . -type d -name __pycache__ -exec rm -rf {} + 2>/dev/null || true
        echo "✅ Clean complete!"
        ;;
    
    testpypi)
        echo "🧪 Preparing TestPyPI upload..."
        echo ""
        echo "Prerequisites:"
        echo "1. Create account at https://test.pypi.org/account/register/"
        echo "2. Generate API token at https://test.pypi.org/manage/account/tokens/"
        echo "3. Configure ~/.pypirc with TestPyPI credentials"
        echo ""
        
        if [ ! -f ~/.pypirc ]; then
            echo "❌ ~/.pypirc not found. Creating template..."
            cat > ~/.pypirc << 'PYPIRC_EOF'
[distutils]
index-servers =
    testpypi
    pypi

[testpypi]
repository = https://test.pypi.org/legacy/
username = __token__
password = pypi_YOUR_TEST_TOKEN_HERE

[pypi]
repository = https://upload.pypi.org/legacy/
username = __token__
password = pypi_YOUR_PROD_TOKEN_HERE
PYPIRC_EOF
            echo "Created ~/.pypirc. Please edit with your tokens."
            exit 1
        fi
        
        echo "📦 Uploading to TestPyPI..."
        twine upload --repository testpypi dist/* --verbose
        echo ""
        echo "✅ Upload complete!"
        echo ""
        echo "Test installation:"
        echo "  pip install --index-url https://test.pypi.org/simple/ $PACKAGE_NAME==$VERSION"
        ;;
    
    pypi)
        echo "🚀 Preparing PyPI upload..."
        echo ""
        echo "Prerequisites:"
        echo "1. Ensure TestPyPI upload was successful"
        echo "2. Verify package at https://test.pypi.org/project/$PACKAGE_NAME/"
        echo "3. Create account at https://pypi.org/account/register/"
        echo "4. Generate API token at https://pypi.org/manage/account/tokens/"
        echo ""
        
        # Safety check - ask for confirmation
        read -p "Are you ready to upload to production PyPI? (yes/no): " CONFIRM
        if [ "$CONFIRM" != "yes" ]; then
            echo "Upload cancelled."
            exit 0
        fi
        
        echo "📦 Uploading to production PyPI..."
        twine upload dist/* --verbose
        echo ""
        echo "✅ Upload complete!"
        echo ""
        echo "Verify at:"
        echo "  https://pypi.org/project/$PACKAGE_NAME/$VERSION/"
        echo ""
        echo "Install from PyPI:"
        echo "  pip install $PACKAGE_NAME"
        ;;
    
    *)
        echo "Unknown command: $COMMAND"
        echo "Use: bash $0 [testpypi|pypi|clean|verify|build]"
        exit 1
        ;;
esac

echo ""
echo "Done! 🎉"
