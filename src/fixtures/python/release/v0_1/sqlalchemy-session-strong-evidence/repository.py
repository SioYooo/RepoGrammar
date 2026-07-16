from sqlalchemy.orm import Session


class UserRepository:
    def __init__(self, session: Session):
        self.session = session

    def list_users(self):
        return self.session.execute("select users")

    def list_accounts(self):
        return self.session.execute("select accounts")

    def list_invoices(self):
        return self.session.execute("select invoices")
