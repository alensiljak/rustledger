// Helper functions for the MCP server

import * as fs from "fs";
import * as path from "path";
import type {
  BeancountError,
  QueryResult,
  ToolResponse,
  ToolArguments,
} from "./types.js";

// Regex to match include directives: include "path/to/file.beancount"
const INCLUDE_REGEX = /^include\s+"([^"]+)"\s*$/gm;

/**
 * Load a beancount file with all its includes resolved.
 *
 * This recursively follows include directives and returns the concatenated
 * source with all includes inlined. Paths in include directives are resolved
 * relative to the file containing the include.
 *
 * @param filePath - The absolute path to the main beancount file
 * @returns The concatenated source with all includes resolved
 * @throws Error if a file cannot be read or circular include detected
 */
export function loadWithIncludes(filePath: string): string {
  const visited = new Set<string>();
  return loadFileRecursive(filePath, visited);
}

function loadFileRecursive(filePath: string, visited: Set<string>): string {
  const absolutePath = path.resolve(filePath);

  // Check for circular includes (only in current recursion stack)
  if (visited.has(absolutePath)) {
    throw new Error(`Circular include detected: ${absolutePath}`);
  }
  visited.add(absolutePath);

  try {
    const source = fs.readFileSync(absolutePath, "utf-8");
    const baseDir = path.dirname(absolutePath);

    // Replace each include directive with the contents of the included file
    return source.replace(INCLUDE_REGEX, (_match, includePath: string) => {
      const includeAbsPath = path.resolve(baseDir, includePath);
      try {
        return loadFileRecursive(includeAbsPath, visited);
      } catch (error) {
        // Re-throw with context about which include failed
        const msg = error instanceof Error ? error.message : String(error);
        throw new Error(`Failed to include "${includePath}" from ${absolutePath}: ${msg}`);
      }
    });
  } finally {
    // Remove from visited after processing to allow same file from different branches
    visited.delete(absolutePath);
  }
}

/**
 * Validate that required arguments are present.
 * Returns a ToolResponse with error if validation fails, null otherwise.
 */
export function validateArgs(
  args: ToolArguments | undefined,
  required: (keyof ToolArguments)[]
): ToolResponse | null {
  const missing: string[] = [];

  for (const key of required) {
    const value = args?.[key];
    // Check for undefined, null, or empty string for string types
    if (value === undefined || value === null) {
      missing.push(key);
    }
  }

  if (missing.length > 0) {
    const argList = missing.join(", ");
    return {
      isError: true,
      content: [
        {
          type: "text",
          text: `Missing required argument${missing.length > 1 ? "s" : ""}: ${argList}`,
        },
      ],
    };
  }

  return null;
}

/**
 * Create an error response.
 */
export function errorResponse(message: string): ToolResponse {
  return {
    isError: true,
    content: [{ type: "text", text: message }],
  };
}

/**
 * Create a success response with text content.
 */
export function textResponse(text: string): ToolResponse {
  return {
    content: [{ type: "text", text }],
  };
}

/**
 * Create a success response with JSON content.
 */
export function jsonResponse(data: unknown): ToolResponse {
  return {
    content: [{ type: "text", text: JSON.stringify(data, null, 2) }],
  };
}

/**
 * Format validation/parse errors for display.
 */
export function formatErrors(errors: BeancountError[]): string {
  return errors
    .map((e) => {
      const loc = e.line ? `:${e.line}${e.column ? `:${e.column}` : ""}` : "";
      return `[${e.severity}]${loc} ${e.message}`;
    })
    .join("\n");
}

/**
 * Format a query result as a table.
 */
export function formatQueryResult(result: QueryResult): string {
  if (!result.columns || result.columns.length === 0) {
    return "No results.";
  }

  const { columns, rows } = result;

  // Calculate column widths
  const widths = columns.map((col, i) => {
    const maxRowWidth = Math.max(
      ...rows.map((row) => formatCell(row[i]).length)
    );
    return Math.max(col.length, maxRowWidth);
  });

  // Format header
  const header = columns.map((col, i) => col.padEnd(widths[i])).join(" | ");
  const separator = widths.map((w) => "-".repeat(w)).join("-+-");

  // Format rows
  const formattedRows = rows.map((row) =>
    row.map((cell, i) => formatCell(cell).padEnd(widths[i])).join(" | ")
  );

  return [header, separator, ...formattedRows].join("\n");
}

/**
 * Format a single cell value for display.
 */
export function formatCell(value: unknown): string {
  if (value === null || value === undefined) {
    return "";
  }
  if (typeof value === "object") {
    // Handle Amount type
    if ("number" in value && "currency" in value) {
      const amount = value as { number: string; currency: string };
      return `${amount.number} ${amount.currency}`;
    }
    // Handle Inventory type
    if ("positions" in value) {
      const inv = value as {
        positions: Array<{ units: { number: string; currency: string } }>;
      };
      return inv.positions
        .map((p) => `${p.units.number} ${p.units.currency}`)
        .join(", ");
    }
    return JSON.stringify(value);
  }
  return String(value);
}
