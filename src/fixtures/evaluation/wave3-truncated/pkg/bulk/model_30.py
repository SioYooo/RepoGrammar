from pydantic import BaseModel


class Item30(BaseModel):
    id: int
    label: str
    quantity: int
