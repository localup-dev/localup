#!/bin/bash

# CLI E2E Test Script
# Tests the binaries and CLI commands to ensure they work for users
# This runs as part of CI to catch CLI issues early

set -e

echo "🧪 CLI E2E Tests"
echo "================"
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Determine the localup binary path
if [ -x /usr/bin/localup ]; then
    LOCALUP_BIN="/usr/bin/localup"
elif [ -x ./target/release/localup ]; then
    LOCALUP_BIN="./target/release/localup"
else
    echo "Error: Could not find localup binary"
    exit 1
fi

echo "Using localup binary: $LOCALUP_BIN"
echo ""

# Helper functions
run_test() {
    local test_name="$1"
    local command="$2"
    local should_fail="${3:-false}"

    # Replace standalone 'localup' commands with the actual binary path
    # Using parameter expansion instead of sed to avoid platform differences
    command="${command//localup/$LOCALUP_BIN}"

    echo -n "Testing: $test_name ... "
    if eval "$command" > /tmp/test_output.txt 2>&1; then
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
            cat /tmp/test_output.txt 2>/dev/null || echo "(no output)"
            return 1
        fi
    fi
}

test_count=0
test_passed=0
test_failed=0

echo "📦 LocalUp Client Binary Tests"
echo "=============================="

# Test 1: localup client --version
test_count=$((test_count + 1))
if run_test "localup --version" "localup --version"; then
    test_passed=$((test_passed + 1))
else
    test_failed=$((test_failed + 1))
fi

# Test 2: localup --help
test_count=$((test_count + 1))
if run_test "localup --help" "localup --help | grep -q 'Usage'"; then
    test_passed=$((test_passed + 1))
else
    test_failed=$((test_failed + 1))
fi

# Test 3: localup connect --help
test_count=$((test_count + 1))
if run_test "localup connect --help" "localup connect --help | grep -q 'connect to'"; then
    test_passed=$((test_passed + 1))
else
    test_failed=$((test_failed + 1))
fi

# Test 4: localup connect without required args (should fail)
test_count=$((test_count + 1))
if run_test "localup connect missing args fails" "localup connect" "true"; then
    test_passed=$((test_passed + 1))
else
    test_failed=$((test_failed + 1))
fi

# Test 5: localup with invalid command (should fail gracefully)
test_count=$((test_count + 1))
if run_test "localup invalid-command fails" "localup invalid-command" "true"; then
    test_passed=$((test_passed + 1))
else
    test_failed=$((test_failed + 1))
fi

echo ""
echo "🔌 LocalUp Relay Subcommand Tests"
echo "=================================="

# Test 6: localup relay --help
test_count=$((test_count + 1))
if run_test "localup relay --help" "localup relay --help | grep -q 'Run as exit node'"; then
    test_passed=$((test_passed + 1))
else
    test_failed=$((test_failed + 1))
fi

# Test 7: localup relay with invalid args (should fail gracefully)
test_count=$((test_count + 1))
if run_test "localup relay invalid-args fails" "localup relay --invalid-flag" "true"; then
    test_passed=$((test_passed + 1))
else
    test_failed=$((test_failed + 1))
fi

echo ""
echo "🤖 LocalUp Agent Subcommand Tests"
echo "=================================="

# Test 8: localup agent --help
test_count=$((test_count + 1))
if run_test "localup agent --help" "localup agent --help | grep -q 'Run as reverse tunnel agent'"; then
    test_passed=$((test_passed + 1))
else
    test_failed=$((test_failed + 1))
fi

# Test 9: localup agent without required args (should fail)
test_count=$((test_count + 1))
if run_test "localup agent missing args fails" "localup agent" "true"; then
    test_passed=$((test_passed + 1))
else
    test_failed=$((test_failed + 1))
fi

echo ""
echo "🖥️  LocalUp Agent-Server Subcommand Tests"
echo "=========================================="

# Test 10: localup agent-server --help
test_count=$((test_count + 1))
if run_test "localup agent-server --help" "localup agent-server --help | grep -q 'Run as agent server'"; then
    test_passed=$((test_passed + 1))
else
    test_failed=$((test_failed + 1))
fi

# Test 11: localup agent-server with invalid args (should fail gracefully)
test_count=$((test_count + 1))
if run_test "localup agent-server invalid-args fails" "localup agent-server --invalid-flag" "true"; then
    test_passed=$((test_passed + 1))
else
    test_failed=$((test_failed + 1))
fi

echo ""
echo "🔐 LocalUp Generate-Token Subcommand Tests"
echo "==========================================="

# Test 11: localup generate-token --help
test_count=$((test_count + 1))
if run_test "localup generate-token --help" "localup generate-token --help | grep -q 'JWT secret'"; then
    test_passed=$((test_passed + 1))
else
    test_failed=$((test_failed + 1))
fi

# Test 12: localup generate-token without required args (should fail)
test_count=$((test_count + 1))
if run_test "localup generate-token missing secret fails" "localup generate-token" "true"; then
    test_passed=$((test_passed + 1))
else
    test_failed=$((test_failed + 1))
fi

# Test 13: localup generate-token with basic args (should succeed)
test_count=$((test_count + 1))
echo -n "Testing: localup generate-token generates token ... "
if $LOCALUP_BIN generate-token --secret 'test-secret' --sub 'myapp' 2>/dev/null | grep -q 'JWT Token generated'; then
    echo -e "${GREEN}✓${NC}"
    test_passed=$((test_passed + 1))
else
    echo -e "${RED}✗${NC}"
    test_failed=$((test_failed + 1))
fi

# Test 14: localup generate-token with reverse tunnel options (should succeed)
test_count=$((test_count + 1))
echo -n "Testing: localup generate-token with reverse tunnel ... "
if $LOCALUP_BIN generate-token --secret 'test-secret' --sub 'myapp' --reverse-tunnel --agent agent-1 2>/dev/null | grep -q 'Reverse tunnel: enabled'; then
    echo -e "${GREEN}✓${NC}"
    test_passed=$((test_passed + 1))
else
    echo -e "${RED}✗${NC}"
    test_failed=$((test_failed + 1))
fi

echo ""
echo "📚 Verifying Consolidated Binary and Subcommands"
echo "==============================================="

# Test 15: Verify localup is executable (use direct PATH since replacement would break this test)
test_count=$((test_count + 1))
echo -n "Testing: localup exists and is executable ... "
if [ -x "$LOCALUP_BIN" ]; then
    echo -e "${GREEN}✓${NC}"
    test_passed=$((test_passed + 1))
else
    echo -e "${RED}✗${NC}"
    test_failed=$((test_failed + 1))
fi

# Test 16: Verify all subcommands are available
test_count=$((test_count + 1))
if run_test "localup has all subcommands" "localup --help | grep -q 'relay' && localup --help | grep -q 'agent' && localup --help | grep -q 'agent-server' && localup --help | grep -q 'generate-token'"; then
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
    echo -e "${GREEN}✓ All CLI tests passed!${NC}"
    exit 0
else
    echo -e "${RED}✗ Some CLI tests failed${NC}"
    exit 1
fi
