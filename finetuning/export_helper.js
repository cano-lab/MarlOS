/**
 * MarlOS Chat Export Helper
 *
 * Run this in the browser DevTools console (F12) to export your chat data.
 * Copy the entire file contents, paste into console, and press Enter.
 */

(async function exportChatData() {
  try {
    console.log('🔄 Starting chat export...');

    // Get the app data directory
    const appDataPath = await window.__TAURI__.path.appDataDir();
    const exportPath = `${appDataPath}/chat_export.jsonl`;

    console.log('📁 Export path:', exportPath);

    // Export conversations
    const result = await window.__TAURI__.core.invoke('export_conversations_for_finetuning', {
      filters: {
        security_tiers: ["open"],  // Only export open-tier conversations
        min_message_count: 2,       // Only conversations with 2+ messages
        limit: 10000                // Export up to 10k conversations
      },
      outputPath: exportPath
    });

    console.log('✅ Export successful!');
    console.log('📊 Stats:', result);
    console.log(`   - Conversations exported: ${result.conversations_exported}`);
    console.log(`   - Total messages: ${result.total_messages}`);
    console.log(`   - Output file: ${result.output_path}`);

    // Also get provider stats
    const stats = await window.__TAURI__.core.invoke('get_export_stats');
    console.log('📈 Available data:', stats);
    console.log(`   - Total objects: ${stats.total_objects}`);
    console.log(`   - By provider:`, stats.by_provider);

    console.log('\n🎉 Your chat data has been exported!');
    console.log(`\nNext steps:`);
    console.log(`1. Copy the file from: ${result.output_path}`);
    console.log(`2. Run in the finetuning folder:`);
    console.log(`   python train_full.py --input "${result.output_path}"`);

    // Show file path in a more user-friendly way
    const copyPath = result.output_path.replace(/\\/g, '/');
    console.log(`\n💡 Quick copy (for Windows):`);
    console.log(`   python train_full.py --input "${copyPath}"`);

    return result;
  } catch (error) {
    console.error('❌ Export failed:', error);
    throw error;
  }
})();
