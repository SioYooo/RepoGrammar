from fastapi import APIRouter

router = APIRouter()


@router.get("/users")
def list_users():
    return []


@router.get("/teams")
def list_teams():
    return []


@router.get("/accounts")
def list_accounts():
    return []


@router.get("/orders")
def list_orders():
    return []


@router.get("/invoices")
def list_invoices():
    return []


@router.get("/products")
def list_products():
    return []


@router.get("/payments")
def list_payments():
    return []


@router.get("/shipments")
def list_shipments():
    return []


@router.get("/customers")
def list_customers():
    return []


@router.get("/vendors")
def list_vendors():
    return []


@router.get("/tickets")
def list_tickets():
    return []


@router.get("/projects")
def list_projects():
    return []


@router.get("/tasks")
def list_tasks():
    return []


@router.get("/comments")
def list_comments():
    return []


@router.get("/labels")
def list_labels():
    return []


@router.get("/milestones")
def list_milestones():
    return []


@router.get("/releases")
def list_releases():
    return []


@router.get("/branches")
def list_branches():
    return []


@router.get("/commits")
def list_commits():
    return []


@router.get("/reviews")
def list_reviews():
    return []


@router.get("/pipelines")
def list_pipelines():
    return []


@router.get("/runners")
def list_runners():
    return []


@router.get("/secrets")
def list_secrets():
    return []


@router.get("/webhooks")
def list_webhooks():
    return []


@router.get("/environments")
def list_environments():
    return []


@router.get("/deployments")
def list_deployments():
    return []


@router.get("/artifacts")
def list_artifacts():
    return []


@router.get("/packages")
def list_packages():
    return []


@router.get("/registries")
def list_registries():
    return []


@router.get("/tokens")
def list_tokens():
    return []
