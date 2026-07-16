// Exact `@nestjs/common` imports resolve controller identity and every route,
// replacing the blocking UNKNOWNs with `nestjs.common.Get` support facts.
import { Controller, Get } from "@nestjs/common";

@Controller("users")
export class UsersController {
  @Get("all")
  findAll() {
    return [];
  }

  @Get("active")
  findActive() {
    return [];
  }

  @Get("archived")
  findArchived() {
    return [];
  }
}
