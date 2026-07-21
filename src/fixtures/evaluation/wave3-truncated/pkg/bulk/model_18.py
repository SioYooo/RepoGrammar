from pydantic import BaseModel


class Item18(BaseModel):
    id: int
    label: str
    quantity: int
