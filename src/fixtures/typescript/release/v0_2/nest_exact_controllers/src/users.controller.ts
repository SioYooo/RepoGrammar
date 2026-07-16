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
