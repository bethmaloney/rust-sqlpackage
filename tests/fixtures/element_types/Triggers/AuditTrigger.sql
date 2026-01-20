CREATE TRIGGER [dbo].[TR_Users_Audit]
ON [dbo].[Users]
AFTER INSERT, UPDATE, DELETE
AS
BEGIN
    SET NOCOUNT ON;
    -- Audit trigger placeholder
END
GO
