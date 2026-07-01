#!/usr/bin/env node
import { Command } from "commander";
import { addCommand } from "./commands/add.js";
import { deleteCommand } from "./commands/delete.js";
import { listCommand } from "./commands/list.js";
import { resolveCommand } from "./commands/resolve.js";
import { keyCommand } from "./commands/key.js";
import { zoneCheckCommand } from "./commands/zone-check.js";
import { zoneExportCommand } from "./commands/zone-export.js";
import { conformanceCommand } from "./commands/conformance.js";
import { refundCommand } from "./commands/refund.js";
import { createBurnCommand } from "./commands/burn.js";

const program = new Command();

program
  .name("nodns")
  .description("Manage DNS records published via Nostr events")
  .version("0.2.0")
  .option("--relay <url>", "Single relay override (default: all configured relays)")
  .option("--zone <zone>", "Default zone", "nodns.shop")
  .option("--sec <key>", "Secret key (nsec, hex)")
  .option("--skip-zone-check", "Skip zone validation");

program.addCommand(addCommand);
program.addCommand(deleteCommand);
program.addCommand(listCommand);
program.addCommand(resolveCommand);
program.addCommand(keyCommand);
program.addCommand(zoneCheckCommand);
program.addCommand(zoneExportCommand);
program.addCommand(conformanceCommand);
program.addCommand(refundCommand);
program.addCommand(createBurnCommand());

program.parse();
