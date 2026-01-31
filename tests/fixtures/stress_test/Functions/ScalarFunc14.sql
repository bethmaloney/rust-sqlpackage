CREATE FUNCTION [dbo].[ScalarFunc14]
(
    @Id INT
)
RETURNS NVARCHAR(100)
AS
BEGIN
    DECLARE @Result NVARCHAR(100);
    
    SELECT @Result = [Name]
    FROM [dbo].[Table15]
    WHERE [Id] = @Id;
    
    RETURN @Result;
END
GO
