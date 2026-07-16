namespace Example.Lookalikes.Controllers;

// A locally declared type that merely shares the ASP.NET Core base name. It is
// not Microsoft.AspNetCore.Mvc.ControllerBase, so it must never anchor a role.
public class ControllerBase
{
}

public class LookalikeController : ControllerBase
{
    [HttpGet("items")]
    public string ListItems()
    {
        return "items";
    }

    [HttpGet("summary")]
    public string GetSummary()
    {
        return "summary";
    }

    [Fact]
    public void AlwaysPasses()
    {
    }
}
