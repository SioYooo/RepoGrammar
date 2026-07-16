using Microsoft.AspNetCore.Mvc;

namespace Example.Catalog.Controllers;

[ApiController]
[Route("api/catalog")]
public class CatalogController : ControllerBase
{
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
}
