// The decorators are local factories, not exact `@nestjs/common` imports,
// so controller identity and every route stay blocking UNKNOWNs.
function Controller(_path: string) {
  return (target: unknown) => target;
}

function Get(_path: string) {
  return (_target: unknown, _key: string) => {};
}

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
