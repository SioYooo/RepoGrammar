namespace Example.Unresolved.Controllers;

// No `using Microsoft.AspNetCore.Mvc;` and no exact base type, so the [HttpGet]
// attributes are lookalikes: each stays a blocking csharp_attribute_binding
// UNKNOWN and no family may form.
public class CatalogController
{
    [HttpGet("items")]
    public string ListItems()
    {
        return "items";
    }

    [HttpGet("items/{id}")]
    public string GetItem(string id)
    {
        return id;
    }

    [HttpGet("summary")]
    public string GetSummary()
    {
        return "summary";
    }
}
