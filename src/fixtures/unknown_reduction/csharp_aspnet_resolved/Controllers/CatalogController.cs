using Microsoft.AspNetCore.Mvc;

namespace Example.Resolved.Controllers;

// Exact same-file using resolves the ASP.NET Core attributes, so each [HttpGet]
// derives a bounded DATAFLOW_DERIVED support fact targeting
// aspnetcore.mvc.HttpGet and the three actions form one family.
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
