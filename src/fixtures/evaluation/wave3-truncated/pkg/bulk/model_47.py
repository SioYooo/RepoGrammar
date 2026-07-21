from pydantic import BaseModel


class Item47(BaseModel):
    id: int
    label: str
    quantity: int
