#!/bin/bash
# Script pour surveiller les changements du token cache Gmail

CACHE_FILE="gmail-token-cache.json"

if [ ! -f "$CACHE_FILE" ]; then
    echo "❌ Token cache file not found: $CACHE_FILE"
    exit 1
fi

echo "🔍 Monitoring Gmail token cache changes"
echo "========================================"
echo ""

# Function to display token info
show_token_info() {
    if command -v jq &> /dev/null; then
        EXPIRES_AT=$(jq -r '.[0].token.expires_at' "$CACHE_FILE" 2>/dev/null)
        
        if [ "$EXPIRES_AT" != "null" ] && [ -n "$EXPIRES_AT" ]; then
            YEAR=$(echo "$EXPIRES_AT" | jq '.[0]')
            DAY=$(echo "$EXPIRES_AT" | jq '.[1]')
            HOUR=$(echo "$EXPIRES_AT" | jq '.[2]')
            MIN=$(echo "$EXPIRES_AT" | jq '.[3]')
            SEC=$(echo "$EXPIRES_AT" | jq '.[4]')
            
            echo "📅 Token expires: Day $DAY of $YEAR at ${HOUR}:$(printf '%02d' $MIN):$(printf '%02d' $SEC) UTC"
            
            # Calculate date from day of year
            if command -v date &> /dev/null; then
                EXPIRE_DATE=$(date -d "$YEAR-01-01 +$((DAY-1)) days $HOUR:$MIN:$SEC UTC" '+%Y-%m-%d %H:%M:%S %Z' 2>/dev/null)
                if [ -n "$EXPIRE_DATE" ]; then
                    echo "   Human readable: $EXPIRE_DATE"
                fi
                
                # Time until expiration
                NOW_EPOCH=$(date +%s)
                EXPIRE_EPOCH=$(date -d "$YEAR-01-01 +$((DAY-1)) days $HOUR:$MIN:$SEC UTC" +%s 2>/dev/null)
                
                if [ -n "$EXPIRE_EPOCH" ]; then
                    TIME_DIFF=$((EXPIRE_EPOCH - NOW_EPOCH))
                    TIME_DIFF_MIN=$((TIME_DIFF / 60))
                    
                    if [ $TIME_DIFF -gt 0 ]; then
                        echo "   ⏱️  Time remaining: $TIME_DIFF_MIN minutes"
                        
                        if [ $TIME_DIFF_MIN -lt 5 ]; then
                            echo "   ⚠️  Token will expire soon! Refresh expected on next API call."
                        elif [ $TIME_DIFF_MIN -lt 15 ]; then
                            echo "   🔶 Token getting old, refresh may happen soon."
                        else
                            echo "   ✅ Token is fresh."
                        fi
                    else
                        echo "   ❌ Token has expired!"
                    fi
                fi
            fi
        fi
    else
        echo "⚠️  jq not installed - showing raw data only"
        cat "$CACHE_FILE"
    fi
}

# Show initial state
echo "Initial state:"
LAST_MODIFIED=$(stat -c %y "$CACHE_FILE" 2>/dev/null || stat -f "%Sm" "$CACHE_FILE" 2>/dev/null)
echo "📁 Cache file last modified: $LAST_MODIFIED"
show_token_info
echo ""

# Monitor for changes
echo "👀 Watching for changes (Ctrl+C to stop)..."
echo "   (Token cache updates only when yup-oauth2 performs an actual token refresh)"
echo ""

PREVIOUS_MODIFIED="$LAST_MODIFIED"
COUNTER=0

while true; do
    sleep 10
    COUNTER=$((COUNTER + 1))
    
    CURRENT_MODIFIED=$(stat -c %y "$CACHE_FILE" 2>/dev/null || stat -f "%Sm" "$CACHE_FILE" 2>/dev/null)
    
    # Show a heartbeat every minute (6 * 10 seconds)
    if [ $((COUNTER % 6)) -eq 0 ]; then
        NOW=$(date '+%Y-%m-%d %H:%M:%S')
        echo "[$NOW] ⏳ Still watching... (no changes)"
    fi
    
    if [ "$CURRENT_MODIFIED" != "$PREVIOUS_MODIFIED" ]; then
        NOW=$(date '+%Y-%m-%d %H:%M:%S')
        echo ""
        echo "========================================="
        echo "[$NOW] 🔄 CACHE FILE CHANGED!"
        echo "========================================="
        echo "📁 New modification time: $CURRENT_MODIFIED"
        echo ""
        show_token_info
        echo ""
        PREVIOUS_MODIFIED="$CURRENT_MODIFIED"
        COUNTER=0
    fi
done
