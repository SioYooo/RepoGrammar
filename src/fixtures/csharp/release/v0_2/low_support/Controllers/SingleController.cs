using Microsoft.AspNetCore.Mvc;

namespace Example.LowSupport.Controllers;

[ApiController]
[Route("api/single")]
public class SingleController : ControllerBase
{
    [HttpGet("only")]
    public IActionResult OnlyAction()
    {
        return Ok();
    }
}
