// `@Controller`/`@Get` are local factories, not `@nestjs/common` imports.
function Controller(_path: string) {
  return (target: unknown) => target;
}

function Get(_path: string) {
  return (_target: unknown, _key: string) => {};
}

@Controller("fake")
export class FakeController {
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
