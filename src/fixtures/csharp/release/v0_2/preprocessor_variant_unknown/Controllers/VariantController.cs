using Microsoft.AspNetCore.Mvc;

namespace Example.Variants.Controllers;

[ApiController]
[Route("api/variant")]
public class VariantController : ControllerBase
{
#if DEBUG
    [HttpGet("items")]
    public IActionResult ListItems()
    {
        return Ok();
    }

    [HttpGet("items/{id}")]
    public IActionResult GetItem(int id)
    {
        return Ok(id);
    }

    [HttpGet("summary")]
    public IActionResult GetSummary()
    {
        return Ok();
    }
#endif
}
