import { client as aliasClient } from "@/lib/client";
import { client as relativeClient } from "./lib/client";
import missing from "@/missing";

const dynamicName = "@/lib/client";
import(dynamicName);

export * from "@/lib/client";

void aliasClient;
void relativeClient;
void missing;
