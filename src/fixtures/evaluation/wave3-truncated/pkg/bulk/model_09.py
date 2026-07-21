from pydantic import BaseModel


class Item09(BaseModel):
    id: int
    label: str
    quantity: int
