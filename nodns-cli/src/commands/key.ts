import { Command } from "commander";
import { generateKeypair, decodeSec } from "../lib/nostr.js";

const generateCommand = new Command("generate")
  .description("Generate a new keypair")
  .action(() => {
    const kp = generateKeypair();
    console.log(`nsec: ${kp.nsec}`);
    console.log(`npub: ${kp.npub}`);
    console.error("\nSave the nsec to your config or use --sec=nsec1...");
  });

const deriveCommand = new Command("derive")
  .description("Derive npub from an nsec or hex secret key")
  .argument("<key>", "Secret key (nsec1... or hex)")
  .action((key: string) => {
    try {
      const kp = decodeSec(key);
      console.log(kp.npub);
    } catch {
      console.error("Error: invalid secret key. Provide an nsec1... or 64-char hex key.");
      process.exit(1);
    }
  });

export const keyCommand = new Command("key")
  .description("Key management");

keyCommand.addCommand(generateCommand);
keyCommand.addCommand(deriveCommand);
