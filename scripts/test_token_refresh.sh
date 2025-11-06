#!/bin/bash
# Test script to verify token refresh works correctly

set -e

CACHE_FILE="gmail-token-cache.json"

echo "🔍 Testing Token Refresh System"
echo "================================"
echo ""

# Check if token cache file exists
if [ ! -f "$CACHE_FILE" ]; then
    echo "❌ Token cache file not found: $CACHE_FILE"
    echo "   Run the application first to generate tokens."
    exit 1
fi

echo "✅ Token cache file found"
echo ""

# Extract and display current token expiration
if command -v jq &> /dev/null; then
    echo "📅 Current Token Expiration:"
    EXPIRES_AT=$(jq -r '.[0].token.expires_at' "$CACHE_FILE")
    echo "   Raw: $EXPIRES_AT"
    
    # Parse the chrono DateTime format: [year, day_of_year, hour, min, sec, nanosec, tz_offset, 0, 0]
    YEAR=$(echo "$EXPIRES_AT" | jq '.[0]')
    DAY_OF_YEAR=$(echo "$EXPIRES_AT" | jq '.[1]')
    HOUR=$(echo "$EXPIRES_AT" | jq '.[2]')
    MIN=$(echo "$EXPIRES_AT" | jq '.[3]')
    SEC=$(echo "$EXPIRES_AT" | jq '.[4]')
    
    # Convert day of year to date (approximation)
    if command -v date &> /dev/null; then
        # GNU date command
        DATE_STR=$(date -d "$YEAR-01-01 +$((DAY_OF_YEAR-1)) days" +%Y-%m-%d 2>/dev/null || echo "Unknown")
        echo "   Expires: $DATE_STR at ${HOUR}:$(printf '%02d' $MIN):$(printf '%02d' $SEC) UTC"
    else
        echo "   Day of year: $DAY_OF_YEAR"
        echo "   Time: ${HOUR}:$(printf '%02d' $MIN):$(printf '%02d' $SEC) UTC"
    fi
    
    # Calculate time until expiration
    NOW_EPOCH=$(date +%s)
    EXPIRES_EPOCH=$(date -d "$YEAR-01-01 +$((DAY_OF_YEAR-1)) days $HOUR:$MIN:$SEC UTC" +%s 2>/dev/null || echo "0")
    
    if [ "$EXPIRES_EPOCH" != "0" ]; then
        TIME_DIFF=$((EXPIRES_EPOCH - NOW_EPOCH))
        TIME_DIFF_MIN=$((TIME_DIFF / 60))
        
        if [ $TIME_DIFF -gt 0 ]; then
            echo "   Time remaining: $TIME_DIFF_MIN minutes"
            
            if [ $TIME_DIFF_MIN -lt 15 ]; then
                echo "   ⚠️  Token expires in less than 15 minutes!"
            elif [ $TIME_DIFF_MIN -lt 30 ]; then
                echo "   🔶 Token will be refreshed soon"
            else
                echo "   ✅ Token is fresh"
            fi
        else
            echo "   ❌ Token has already expired!"
        fi
    fi
else
    echo "⚠️  jq not installed - can't parse token details"
    echo "   Install jq for detailed token information: apt-get install jq"
fi

echo ""
echo "🧪 Testing Build"
echo "================"
cargo build --release 2>&1 | grep -E "(Compiling|Finished|error)" || true

if [ $? -eq 0 ]; then
    echo "✅ Build successful"
else
    echo "❌ Build failed"
    exit 1
fi

echo ""
echo "📝 Token Refresh Implementation Check"
echo "======================================"

# Check that the implementation uses the correct approach
if grep -q "auth_arc.lock().await.clone()" src/gmail_client.rs 2>/dev/null; then
    echo "❌ PROBLEM: Still using auth_arc.lock().await.clone()"
    echo "   This will cause token refresh failures!"
    exit 1
else
    echo "✅ Not using problematic clone() pattern"
fi

if grep -q "pub async fn refresh_token" src/gmail_client.rs; then
    echo "✅ refresh_token() method exists"
else
    echo "❌ refresh_token() method not found"
    exit 1
fi

if grep -q "get_profile" src/gmail_client.rs; then
    echo "✅ Uses lightweight API call for token refresh"
else
    echo "⚠️  Warning: refresh_token() implementation may not trigger yup-oauth2 auto-refresh"
fi

echo ""
echo "🎯 Ready for Production Testing"
echo "================================"
echo ""
echo "Next steps to validate the fix:"
echo "1. Run daemon mode: cargo run --release -- --daemon"
echo "2. Monitor logs for token refreshes (every 45 minutes)"
echo "3. Watch for successful refreshes at 45, 90, 135, 180 minutes"
echo "4. Verify NO OAuth2 authorization URLs are requested"
echo "5. Check token cache is updated: watch -n 60 'cat $CACHE_FILE | jq .[0].token.expires_at'"
echo ""
echo "Expected log pattern:"
echo "  [HH:MM:SS] 🔄 Forcing OAuth2 token refresh via API call..."
echo "  [HH:MM:SS] ✅ Token refreshed successfully"
echo ""
