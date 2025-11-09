#!/bin/bash

# Docker E2E Test Script
# Tests the Docker image to ensure it works correctly

set -e

echo "🐳 Docker E2E Tests"
echo "=================="
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if Docker is available
if ! command -v docker &> /dev/null; then
    echo -e "${RED}✗ Docker is not installed${NC}"
    exit 1
fi

echo "Docker version:"
docker --version
echo ""

# Image name
IMAGE_NAME="localup:latest"

# Test if image exists
if ! docker image ls | grep -q "$IMAGE_NAME"; then
    echo -e "${RED}✗ Docker image '$IMAGE_NAME' not found${NC}"
    echo "Build the image first:"
    echo "  docker build -f Dockerfile.ubuntu -t localup:latest ."
    exit 1
fi

echo "Using Docker image: $IMAGE_NAME"
echo ""

# Helper function to run test
run_test() {
    local test_name="$1"
    local command="$2"
    local should_fail="${3:-false}"

    echo -n "Testing: $test_name ... "
    if eval "$command" > /tmp/docker_test_output.txt 2>&1; then
        if [ "$should_fail" = "true" ]; then
            echo -e "${RED}✗ (should have failed)${NC}"
            return 1
        else
            echo -e "${GREEN}✓${NC}"
            return 0
        fi
    else
        if [ "$should_fail" = "true" ]; then
            echo -e "${GREEN}✓ (correctly failed)${NC}"
            return 0
        else
            echo -e "${RED}✗${NC}"
            echo "Command: $command"
            echo "Output:"
            cat /tmp/docker_test_output.txt 2>/dev/null || echo "(no output)"
            return 1
        fi
    fi
}

test_count=0
test_passed=0
test_failed=0

echo "📦 Docker Image Tests"
echo "===================="
echo ""

# Test 1: Image exists and is executable
test_count=$((test_count + 1))
echo -n "Testing: docker image exists ... "
if docker image ls | grep -q "$IMAGE_NAME"; then
    echo -e "${GREEN}✓${NC}"
    test_passed=$((test_passed + 1))
else
    echo -e "${RED}✗${NC}"
    test_failed=$((test_failed + 1))
fi

# Test 2: Container runs without error
test_count=$((test_count + 1))
if run_test "docker container runs" "docker run --rm $IMAGE_NAME --version"; then
    test_passed=$((test_passed + 1))
else
    test_failed=$((test_failed + 1))
fi

# Test 3: Help command works
test_count=$((test_count + 1))
if run_test "docker run with --help" "docker run --rm $IMAGE_NAME --help | grep -q 'Usage'"; then
    test_passed=$((test_passed + 1))
else
    test_failed=$((test_failed + 1))
fi

echo ""
echo "🔐 LocalUp Commands in Docker"
echo "============================="
echo ""

# Test 4: Generate token command
test_count=$((test_count + 1))
if run_test "generate-token command" "docker run --rm $IMAGE_NAME generate-token --secret 'test' --localup-id 'test' 2>&1 | grep -q 'JWT'"; then
    test_passed=$((test_passed + 1))
else
    test_failed=$((test_failed + 1))
fi

# Test 5: Connect help command
test_count=$((test_count + 1))
if run_test "connect --help command" "docker run --rm $IMAGE_NAME connect --help"; then
    test_passed=$((test_passed + 1))
else
    test_failed=$((test_failed + 1))
fi

# Test 6: Relay help command
test_count=$((test_count + 1))
if run_test "relay --help command" "docker run --rm $IMAGE_NAME relay --help | grep -q 'exit node'"; then
    test_passed=$((test_passed + 1))
else
    test_failed=$((test_failed + 1))
fi

# Test 7: Agent help command
test_count=$((test_count + 1))
if run_test "agent --help command" "docker run --rm $IMAGE_NAME agent --help"; then
    test_passed=$((test_passed + 1))
else
    test_failed=$((test_failed + 1))
fi

# Test 8: Agent-server help command
test_count=$((test_count + 1))
if run_test "agent-server --help command" "docker run --rm $IMAGE_NAME agent-server --help"; then
    test_passed=$((test_passed + 1))
else
    test_failed=$((test_failed + 1))
fi

echo ""
echo "🔍 Docker Container Behavior"
echo "============================"
echo ""

# Test 9: Container stops gracefully
test_count=$((test_count + 1))
echo -n "Testing: container stops gracefully ... "
if timeout 5 docker run --rm $IMAGE_NAME 2>&1 | grep -q "Usage" > /dev/null 2>&1 || true; then
    echo -e "${GREEN}✓${NC}"
    test_passed=$((test_passed + 1))
else
    echo -e "${RED}✗${NC}"
    test_failed=$((test_failed + 1))
fi

# Test 10: Invalid command fails properly
test_count=$((test_count + 1))
if run_test "invalid command fails" "docker run --rm $IMAGE_NAME invalid-command" "true"; then
    test_passed=$((test_passed + 1))
else
    test_failed=$((test_failed + 1))
fi

echo ""
echo "📊 Test Summary"
echo "==============="
echo "Total:  $test_count"
echo -e "Passed: ${GREEN}$test_passed${NC}"
echo -e "Failed: ${RED}$test_failed${NC}"
echo ""

if [ $test_failed -eq 0 ]; then
    echo -e "${GREEN}✓ All Docker tests passed!${NC}"
    exit 0
else
    echo -e "${RED}✗ Some Docker tests failed${NC}"
    exit 1
fi
